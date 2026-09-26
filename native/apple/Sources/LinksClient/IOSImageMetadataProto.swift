import Foundation

/// Small protobuf bridge for the private image metadata carried inside MLS.
/// The native app does not depend on generated Swift protobuf code.
public extension IOSImageMetadata {
    init(protobuf: Data) throws {
        var reader = IOSImageProtoReader(data: protobuf)
        var attachmentID: String?
        var mimeType: String?
        var ciphertextSizeBytes: UInt64?
        var contentKey: Data?
        var nonce: Data?
        var ciphertextSHA256: Data?
        var width: Int?
        var height: Int?
        var blurHash: String?

        while !reader.isAtEnd {
            let tag = try reader.readVarint()
            let field = Int(tag >> 3)
            let wireType = Int(tag & 7)
            guard field > 0 else { throw IOSImageError.invalidMetadata }
            switch field {
            case 1:
                attachmentID = try reader.readString(wireType: wireType)
            case 2:
                mimeType = try reader.readString(wireType: wireType)
            case 3:
                ciphertextSizeBytes = try reader.readVarint(wireType: wireType)
            case 4:
                contentKey = try reader.readBytes(wireType: wireType)
            case 5:
                nonce = try reader.readBytes(wireType: wireType)
            case 6:
                ciphertextSHA256 = try reader.readBytes(wireType: wireType)
            case 7:
                width = try reader.readInt(wireType: wireType)
            case 8:
                height = try reader.readInt(wireType: wireType)
            case 10:
                blurHash = try reader.readString(wireType: wireType)
            case 9, 11, 12, 13, 14:
                throw IOSImageError.invalidMetadata
            default:
                try reader.skip(wireType: wireType)
            }
        }

        guard let attachmentID, let mimeType, let ciphertextSizeBytes,
              let contentKey, let nonce, let ciphertextSHA256,
              let width, let height, let blurHash else {
            throw IOSImageError.invalidMetadata
        }
        try self.init(
            attachmentID: attachmentID,
            mimeType: mimeType,
            ciphertextSizeBytes: ciphertextSizeBytes,
            contentKey: contentKey,
            nonce: nonce,
            ciphertextSHA256: ciphertextSHA256,
            width: width,
            height: height,
            blurHash: blurHash)
    }

    func protobuf() -> Data {
        var writer = IOSImageProtoWriter()
        writer.append(string: attachmentID, field: 1)
        writer.append(string: mimeType, field: 2)
        writer.append(varint: ciphertextSizeBytes, field: 3)
        writer.append(bytes: contentKey, field: 4)
        writer.append(bytes: nonce, field: 5)
        writer.append(bytes: ciphertextSHA256, field: 6)
        writer.append(varint: UInt64(width), field: 7)
        writer.append(varint: UInt64(height), field: 8)
        writer.append(string: blurHash, field: 10)
        return writer.data
    }
}

private struct IOSImageProtoReader {
    private let data: Data
    private var offset = 0

    init(data: Data) {
        self.data = data
    }

    var isAtEnd: Bool { offset == data.count }

    mutating func readVarint(wireType: Int? = nil) throws -> UInt64 {
        if let wireType, wireType != 0 { throw IOSImageError.invalidMetadata }
        var value: UInt64 = 0
        for shift in stride(from: 0, through: 63, by: 7) {
            guard offset < data.count else { throw IOSImageError.invalidMetadata }
            let byte = data[offset]
            offset += 1
            if shift == 63 && byte > 1 { throw IOSImageError.invalidMetadata }
            value |= UInt64(byte & 0x7f) << shift
            if byte & 0x80 == 0 { return value }
        }
        throw IOSImageError.invalidMetadata
    }

    mutating func readBytes(wireType: Int) throws -> Data {
        guard wireType == 2 else { throw IOSImageError.invalidMetadata }
        let length = try readVarint()
        guard length <= UInt64(data.count - offset) else {
            throw IOSImageError.invalidMetadata
        }
        let end = offset + Int(length)
        defer { offset = end }
        return data.subdata(in: offset..<end)
    }

    mutating func readString(wireType: Int) throws -> String {
        guard let value = String(data: try readBytes(wireType: wireType), encoding: .utf8) else {
            throw IOSImageError.invalidMetadata
        }
        return value
    }

    mutating func readInt(wireType: Int) throws -> Int {
        let value = try readVarint(wireType: wireType)
        guard value <= UInt64(Int.max) else { throw IOSImageError.invalidMetadata }
        return Int(value)
    }

    mutating func skip(wireType: Int) throws {
        switch wireType {
        case 0:
            _ = try readVarint()
        case 1:
            try advance(8)
        case 2:
            let length = try readVarint()
            guard length <= UInt64(Int.max) else { throw IOSImageError.invalidMetadata }
            try advance(Int(length))
        case 5:
            try advance(4)
        default:
            throw IOSImageError.invalidMetadata
        }
    }

    private mutating func advance(_ count: Int) throws {
        guard count >= 0, count <= data.count - offset else {
            throw IOSImageError.invalidMetadata
        }
        offset += count
    }
}

private struct IOSImageProtoWriter {
    private(set) var data = Data()

    mutating func append(string: String, field: UInt64) {
        append(bytes: Data(string.utf8), field: field)
    }

    mutating func append(bytes: Data, field: UInt64) {
        appendVarint((field << 3) | 2)
        appendVarint(UInt64(bytes.count))
        data.append(bytes)
    }

    mutating func append(varint: UInt64, field: UInt64) {
        appendVarint(field << 3)
        appendVarint(varint)
    }

    private mutating func appendVarint(_ value: UInt64) {
        var value = value
        while value >= 0x80 {
            data.append(UInt8(value & 0x7f) | 0x80)
            value >>= 7
        }
        data.append(UInt8(value))
    }
}
