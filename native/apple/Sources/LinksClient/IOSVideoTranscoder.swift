#if os(iOS)
import AVFoundation
import CoreMedia
import CoreVideo
import Foundation
import VideoToolbox

public enum IOSVideoCodec: Sendable {
    case h264
    case hevc

    fileprivate var codecType: CMVideoCodecType {
        switch self {
        case .h264: return kCMVideoCodecType_H264
        case .hevc: return kCMVideoCodecType_HEVC
        }
    }

    public var mimeType: String {
        switch self {
        case .h264: return "video/avc"
        case .hevc: return "video/hevc"
        }
    }
}

public struct IOSVideoProfile: Sendable {
    public let codec: IOSVideoCodec
    public let width: Int
    public let height: Int
    public let bitrateBps: Int
    public let frameRate: Int

    public static func hd720p(codec: IOSVideoCodec) -> Self {
        try! Self(codec: codec, width: 1_280, height: 720,
                  bitrateBps: 1_500_000, frameRate: 30)
    }

    public static func fullHd1080p(codec: IOSVideoCodec) -> Self {
        try! Self(codec: codec, width: 1_920, height: 1_080,
                  bitrateBps: 3_000_000, frameRate: 30)
    }

    public init(codec: IOSVideoCodec, width: Int, height: Int,
                bitrateBps: Int, frameRate: Int = 30) throws {
        guard (width == 1_280 && height == 720)
                || (width == 1_920 && height == 1_080),
              bitrateBps > 0, (1...60).contains(frameRate) else {
            throw IOSVideoError.invalidProfile
        }
        self.codec = codec
        self.width = width
        self.height = height
        self.bitrateBps = bitrateBps
        self.frameRate = frameRate
    }
}

public struct IOSTranscodedVideo: Sendable {
    public let fileURL: URL
    public let codec: IOSVideoCodec
    public let width: Int
    public let height: Int
    public let durationMs: UInt64
    public let hasAudio: Bool
}

public final class IOSVideoTranscoder {
    public static let maximumInputBytes: UInt64 = 256 * 1024 * 1024

    public init() {}

    /// Decode through AVAssetReader, scale through a video composition, and
    /// encode through a hardware-required VideoToolbox session into MP4.
    public func transcode(source: URL, destination: URL,
                          profile: IOSVideoProfile) async throws -> IOSTranscodedVideo {
        guard FileManager.default.fileExists(atPath: source.path),
              let attributes = try? FileManager.default.attributesOfItem(atPath: source.path),
              let inputSize = (attributes[.size] as? NSNumber)?.uint64Value,
              inputSize > 0, inputSize <= Self.maximumInputBytes,
              source.standardizedFileURL != destination.standardizedFileURL,
              !FileManager.default.fileExists(atPath: destination.path) else {
            throw IOSVideoError.invalidInput
        }
        guard Self.hasHardwareEncoder(profile.codec.codecType) else {
            throw IOSVideoError.hardwareUnavailable
        }

        let asset = AVURLAsset(url: source)
        let duration = try await asset.load(.duration)
        guard let videoTrack = try await asset.loadTracks(withMediaType: .video).first else {
            throw IOSVideoError.invalidInput
        }
        let preferredTransform = try await videoTrack.load(.preferredTransform)
        let reader = try AVAssetReader(asset: asset)
        let videoOutput = makeVideoOutput(
            duration: duration, track: videoTrack,
            preferredTransform: preferredTransform, profile: profile)
        reader.add(videoOutput)

        let audioTrack = try await asset.loadTracks(withMediaType: .audio).first
        let audioOutput = audioTrack.map { AVAssetReaderTrackOutput(track: $0, outputSettings: nil) }
        if let audioOutput { reader.add(audioOutput) }

        let writer = try AVAssetWriter(outputURL: destination, fileType: .mp4)
        let videoInput = AVAssetWriterInput(mediaType: .video, outputSettings: nil)
        videoInput.expectsMediaDataInRealTime = false
        guard writer.canAdd(videoInput) else { throw IOSVideoError.writerUnavailable }
        writer.add(videoInput)

        let audioInput: AVAssetWriterInput?
        if audioTrack != nil {
            let input = AVAssetWriterInput(mediaType: .audio, outputSettings: nil)
            input.expectsMediaDataInRealTime = false
            guard writer.canAdd(input) else { throw IOSVideoError.writerUnavailable }
            writer.add(input)
            audioInput = input
        } else {
            audioInput = nil
        }

        guard reader.startReading(), writer.startWriting() else {
            throw IOSVideoError.readerWriterUnavailable
        }
        writer.startSession(atSourceTime: .zero)

        let context = VideoCompressionContext(input: videoInput)
        var session: VTCompressionSession?
        var specification: CFDictionary?
        if #available(iOS 17.4, *) {
            specification = [
                kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder as String: true
            ] as CFDictionary
        }
        let createStatus = VTCompressionSessionCreate(
            allocator: nil,
            width: Int32(profile.width),
            height: Int32(profile.height),
            codecType: profile.codec.codecType,
            encoderSpecification: specification,
            imageBufferAttributes: nil,
            compressedDataAllocator: nil,
            outputCallback: videoCompressionCallback,
            refcon: Unmanaged.passUnretained(context).toOpaque(),
            compressionSessionOut: &session)
        guard createStatus == noErr, let session else {
            writer.cancelWriting()
            throw IOSVideoError.hardwareUnavailable
        }
        defer {
            VTCompressionSessionInvalidate(session)
            if reader.status == .reading { reader.cancelReading() }
            if writer.status == .writing { writer.cancelWriting() }
        }

        try setCompressionProperties(session, profile: profile)
        guard VTCompressionSessionPrepareToEncodeFrames(session) == noErr else {
            throw IOSVideoError.compressionFailed
        }

        var nextAudio = audioOutput?.copyNextSampleBuffer()
        while let sample = videoOutput.copyNextSampleBuffer() {
            let timestamp = CMSampleBufferGetPresentationTimeStamp(sample)
            try appendAudio(&nextAudio, output: audioOutput, input: audioInput,
                            through: timestamp)
            guard let imageBuffer = CMSampleBufferGetImageBuffer(sample) else {
                throw IOSVideoError.invalidInput
            }
            var encodeFlags = VTEncodeInfoFlags()
            let status = VTCompressionSessionEncodeFrame(
                session,
                imageBuffer: imageBuffer,
                presentationTimeStamp: timestamp,
                duration: CMSampleBufferGetDuration(sample),
                frameProperties: nil,
                sourceFrameRefcon: nil,
                infoFlagsOut: &encodeFlags)
            guard status == noErr else { throw IOSVideoError.compressionFailed }
            if let error = context.failure { throw error }
        }

        guard VTCompressionSessionCompleteFrames(
            session, untilPresentationTimeStamp: .invalid) == noErr else {
            throw IOSVideoError.compressionFailed
        }
        if let error = context.failure { throw error }
        try appendAudio(&nextAudio, output: audioOutput, input: audioInput, through: .positiveInfinity)
        videoInput.markAsFinished()
        audioInput?.markAsFinished()
        await writer.finishWriting()
        guard writer.status == .completed else { throw IOSVideoError.writerUnavailable }

        let seconds = CMTimeGetSeconds(duration)
        guard seconds.isFinite, seconds > 0 else { throw IOSVideoError.invalidInput }
        return IOSTranscodedVideo(
            fileURL: destination,
            codec: profile.codec,
            width: profile.width,
            height: profile.height,
            durationMs: UInt64(seconds * 1_000),
            hasAudio: audioInput != nil)
    }

    private func makeVideoOutput(duration: CMTime, track: AVAssetTrack,
                                 preferredTransform: CGAffineTransform,
                                 profile: IOSVideoProfile) -> AVAssetReaderVideoCompositionOutput {
        let output = AVAssetReaderVideoCompositionOutput(
            videoTracks: [track],
            videoSettings: [
                kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA
            ])
        let composition = AVMutableVideoComposition()
        composition.renderSize = CGSize(width: profile.width, height: profile.height)
        composition.frameDuration = CMTime(value: 1, timescale: CMTimeScale(profile.frameRate))
        let instruction = AVMutableVideoCompositionInstruction()
        instruction.timeRange = CMTimeRange(start: .zero, duration: duration)
        let layer = AVMutableVideoCompositionLayerInstruction(assetTrack: track)
        layer.setTransform(preferredTransform, at: .zero)
        instruction.layerInstructions = [layer]
        composition.instructions = [instruction]
        output.videoComposition = composition
        return output
    }

    private func setCompressionProperties(_ session: VTCompressionSession,
                                          profile: IOSVideoProfile) throws {
        let properties: [(CFString, CFTypeRef)] = [
            (kVTCompressionPropertyKey_RealTime, kCFBooleanFalse),
            (kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse),
            (kVTCompressionPropertyKey_AverageBitRate, NSNumber(value: profile.bitrateBps)),
            (kVTCompressionPropertyKey_MaxKeyFrameInterval, NSNumber(value: profile.frameRate * 2)),
            (kVTCompressionPropertyKey_ExpectedFrameRate, NSNumber(value: profile.frameRate)),
            (kVTCompressionPropertyKey_DataRateLimits,
             [NSNumber(value: profile.bitrateBps / 8), NSNumber(value: 1)] as CFArray)
        ]
        for (key, value) in properties
                where VTSessionSetProperty(session, key: key, value: value) != noErr {
            throw IOSVideoError.compressionFailed
        }
    }

    private static func hasHardwareEncoder(_ codec: CMVideoCodecType) -> Bool {
        var encoders: CFArray?
        guard VTCopyVideoEncoderList(nil, &encoders) == noErr,
              let encoders = encoders as? [[String: Any]] else {
            return false
        }
        return encoders.contains { encoder in
            guard let codecType = (encoder[kVTVideoEncoderList_CodecType as String]
                as? NSNumber)?.uint32Value,
                  let hardware = encoder[kVTVideoEncoderList_IsHardwareAccelerated as String]
                    as? NSNumber else {
                return false
            }
            return codecType == codec && hardware.boolValue
        }
    }

    private func appendAudio(_ nextAudio: inout CMSampleBuffer?,
                             output: AVAssetReaderTrackOutput?,
                             input: AVAssetWriterInput?, through timestamp: CMTime) throws {
        guard let output, let input else { return }
        while let sample = nextAudio,
              CMTimeCompare(CMSampleBufferGetPresentationTimeStamp(sample), timestamp) <= 0 {
            guard input.isReadyForMoreMediaData, input.append(sample) else {
                throw IOSVideoError.writerUnavailable
            }
            nextAudio = output.copyNextSampleBuffer()
        }
    }
}

private final class VideoCompressionContext {
    let input: AVAssetWriterInput
    var failure: IOSVideoError?

    init(input: AVAssetWriterInput) {
        self.input = input
    }
}

private func videoCompressionCallback(
    outputCallbackRefCon: UnsafeMutableRawPointer?,
    sourceFrameRefCon: UnsafeMutableRawPointer?,
    status: OSStatus,
    infoFlags: VTEncodeInfoFlags,
    sampleBuffer: CMSampleBuffer?) {
    guard let outputCallbackRefCon else { return }
    let context = Unmanaged<VideoCompressionContext>
        .fromOpaque(outputCallbackRefCon).takeUnretainedValue()
    guard status == noErr, let sampleBuffer,
          CMSampleBufferDataIsReady(sampleBuffer),
          context.input.isReadyForMoreMediaData,
          context.input.append(sampleBuffer) else {
        context.failure = .compressionFailed
        return
    }
}

public enum IOSVideoError: Error {
    case invalidInput
    case invalidProfile
    case hardwareUnavailable
    case readerWriterUnavailable
    case writerUnavailable
    case compressionFailed
}
#endif
