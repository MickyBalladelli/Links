package ai.links.app;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** Moves the MP4 movie box before media data and fixes absolute chunk offsets. */
final class Mp4FastStart {
    private static final long UINT32_MAX = 0xffff_ffffL;
    private static final int COPY_BUFFER_SIZE = 64 * 1024;
    private static final int MAX_REWRITE_PASSES = 8;

    private Mp4FastStart() {}

    static void rewrite(File source, File destination) throws IOException {
        if (source == null || destination == null || !source.isFile()
                || source.length() == 0 || source.equals(destination) || destination.exists())
            throw new IOException("Invalid MP4 faststart files");
        File parent = destination.getAbsoluteFile().getParentFile();
        if (parent == null || !parent.isDirectory())
            throw new IOException("Missing MP4 faststart directory");

        List<Box> boxes = readTopLevelBoxes(source);
        int moovIndex = findBox(boxes, "moov");
        int mdatIndex = findBox(boxes, "mdat");
        if (moovIndex < 0 || mdatIndex < 0)
            throw new IOException("MP4 moov or mdat box missing");

        File temporary = File.createTempFile(".links-faststart-", ".mp4", parent);
        try {
            if (boxes.get(moovIndex).offset < boxes.get(mdatIndex).offset) {
                copyFile(source, temporary);
            } else {
                rewriteWithMoovFirst(source, temporary, boxes, moovIndex);
            }
            if (!temporary.renameTo(destination))
                throw new IOException("Cannot publish faststart MP4");
        } finally {
            if (temporary.exists()) temporary.delete();
        }
    }

    private static void rewriteWithMoovFirst(File source, File destination,
                                              List<Box> boxes, int moovIndex) throws IOException {
        Box moov = boxes.get(moovIndex);
        byte[] originalMoov = readBox(source, moov);
        byte[] candidate = originalMoov;
        byte[] rewrittenMoov = null;
        for (int pass = 0; pass < MAX_REWRITE_PASSES; pass++) {
            byte[] next = rewriteBox(originalMoov, 0, originalMoov.length, candidate.length);
            if (next.length == candidate.length) {
                rewrittenMoov = next;
                break;
            }
            candidate = next;
        }
        if (rewrittenMoov == null)
            throw new IOException("MP4 chunk offset rewrite did not converge");

        try (RandomAccessFile input = new RandomAccessFile(source, "r");
             BufferedOutputStream output = new BufferedOutputStream(
                     new FileOutputStream(destination))) {
            int ftypIndex = findBox(boxes, "ftyp");
            if (ftypIndex == 0) {
                copyBox(input, output, boxes.get(ftypIndex));
                output.write(rewrittenMoov);
                for (int index = 1; index < boxes.size(); index++) {
                    if (index != moovIndex) copyBox(input, output, boxes.get(index));
                }
            } else {
                output.write(rewrittenMoov);
                for (int index = 0; index < boxes.size(); index++) {
                    if (index != moovIndex) copyBox(input, output, boxes.get(index));
                }
            }
        }
    }

    private static byte[] rewriteBox(byte[] data, int start, int limit, long offsetDelta)
            throws IOException {
        BoxHeader header = readBoxHeader(data, start, limit);
        int end = start + checkedInt(header.size, "MP4 box too large");
        if (end > limit) throw new IOException("MP4 child box exceeds parent");
        if ("stco".equals(header.type) || "co64".equals(header.type))
            return rewriteChunkOffsets(data, header, end, offsetDelta);
        if (!isContainer(header.type))
            return Arrays.copyOfRange(data, start, end);

        int childStart = header.payloadStart;
        if ("meta".equals(header.type)) childStart += 4;
        if (childStart > end) throw new IOException("Invalid MP4 metadata box");
        ByteArrayOutputStream payload = new ByteArrayOutputStream(end - header.payloadStart);
        if (childStart > header.payloadStart)
            payload.write(data, header.payloadStart, childStart - header.payloadStart);
        int cursor = childStart;
        while (cursor < end) {
            BoxHeader childHeader = readBoxHeader(data, cursor, end);
            byte[] child = rewriteBox(data, cursor, end, offsetDelta);
            payload.write(child);
            cursor += checkedInt(childHeader.size, "MP4 child box too large");
        }
        if (cursor != end) throw new IOException("Invalid MP4 child box boundary");
        return makeBox(header.type, payload.toByteArray());
    }

    private static byte[] rewriteChunkOffsets(byte[] data, BoxHeader header, int end,
                                               long offsetDelta) throws IOException {
        int entryWidth = "co64".equals(header.type) ? 8 : 4;
        int payloadLength = end - header.payloadStart;
        if (payloadLength < 8) throw new IOException("Invalid MP4 chunk offset box");
        long entryCount = readUInt32(data, header.payloadStart + 4);
        long entriesLength = entryCount * entryWidth;
        if (entryCount > Integer.MAX_VALUE
                || entriesLength > payloadLength - 8)
            throw new IOException("Invalid MP4 chunk offset count");

        boolean upgradeToCo64 = false;
        long[] offsets = new long[(int) entryCount];
        for (int index = 0; index < offsets.length; index++) {
            int position = header.payloadStart + 8 + index * entryWidth;
            long offset = entryWidth == 4 ? readUInt32(data, position) : readUInt64(data, position);
            offsets[index] = addOffset(offset, offsetDelta);
            if (entryWidth == 4 && offsets[index] > UINT32_MAX) upgradeToCo64 = true;
        }

        int trailingLength = payloadLength - 8 - (int) entriesLength;
        if (!upgradeToCo64) {
            byte[] payload = Arrays.copyOfRange(data, header.payloadStart, end);
            for (int index = 0; index < offsets.length; index++)
                writeUInt(payload, 8 + index * entryWidth, offsets[index], entryWidth);
            return makeBox(header.type, payload);
        }

        int newPayloadLength = checkedInt(8L + offsets.length * 8L + trailingLength,
                "MP4 co64 box too large");
        byte[] payload = new byte[newPayloadLength];
        System.arraycopy(data, header.payloadStart, payload, 0, 8);
        for (int index = 0; index < offsets.length; index++)
            writeUInt(payload, 8 + index * 8, offsets[index], 8);
        if (trailingLength > 0) {
            System.arraycopy(data, header.payloadStart + 8 + (int) entriesLength,
                    payload, 8 + offsets.length * 8, trailingLength);
        }
        return makeBox("co64", payload);
    }

    private static long addOffset(long offset, long delta) throws IOException {
        if (offset < 0 || delta < 0 || offset > Long.MAX_VALUE - delta)
            throw new IOException("MP4 chunk offset overflow");
        return offset + delta;
    }

    private static byte[] makeBox(String type, byte[] payload) throws IOException {
        byte[] typeBytes = type.getBytes(StandardCharsets.US_ASCII);
        if (typeBytes.length != 4) throw new IOException("Invalid MP4 box type");
        long shortSize = 8L + payload.length;
        int headerSize = shortSize <= UINT32_MAX ? 8 : 16;
        long totalSize = shortSize + (headerSize - 8);
        int outputSize = checkedInt(totalSize, "MP4 box too large");
        byte[] box = new byte[outputSize];
        if (headerSize == 8) {
            writeUInt(box, 0, totalSize, 4);
        } else {
            writeUInt(box, 0, 1, 4);
            writeUInt64(box, 8, totalSize);
        }
        System.arraycopy(typeBytes, 0, box, 4, 4);
        System.arraycopy(payload, 0, box, headerSize, payload.length);
        return box;
    }

    private static boolean isContainer(String type) {
        return "moov".equals(type) || "trak".equals(type) || "mdia".equals(type)
                || "minf".equals(type) || "stbl".equals(type) || "edts".equals(type)
                || "dinf".equals(type) || "udta".equals(type) || "meta".equals(type)
                || "ilst".equals(type) || "mvex".equals(type) || "moof".equals(type)
                || "traf".equals(type) || "mfra".equals(type) || "skip".equals(type)
                || "wave".equals(type) || "sinf".equals(type) || "schi".equals(type)
                || "tref".equals(type) || "ipro".equals(type);
    }

    private static List<Box> readTopLevelBoxes(File file) throws IOException {
        List<Box> boxes = new ArrayList<>();
        long fileLength = file.length();
        try (RandomAccessFile input = new RandomAccessFile(file, "r")) {
            long offset = 0;
            while (offset < fileLength) {
                if (fileLength - offset < 8) throw new IOException("Truncated MP4 box");
                input.seek(offset);
                long size = readUInt32(input);
                byte[] typeBytes = new byte[4];
                input.readFully(typeBytes);
                int headerSize = 8;
                if (size == 1) {
                    size = input.readLong();
                    headerSize = 16;
                } else if (size == 0) {
                    size = fileLength - offset;
                }
                if (size < headerSize || size > fileLength - offset)
                    throw new IOException("Invalid MP4 box size");
                boxes.add(new Box(offset, size, headerSize,
                        new String(typeBytes, StandardCharsets.US_ASCII)));
                offset += size;
            }
        }
        return boxes;
    }

    private static BoxHeader readBoxHeader(byte[] data, int start, int limit) throws IOException {
        if (start < 0 || limit - start < 8) throw new IOException("Truncated MP4 child box");
        long size = readUInt32(data, start);
        String type = new String(data, start + 4, 4, StandardCharsets.US_ASCII);
        int headerSize = 8;
        if (size == 1) {
            if (limit - start < 16) throw new IOException("Truncated MP4 extended box");
            size = readUInt64(data, start + 8);
            headerSize = 16;
        } else if (size == 0) {
            size = limit - start;
        }
        if (size < headerSize || size > limit - start)
            throw new IOException("Invalid MP4 child box size");
        return new BoxHeader(size, headerSize, start + headerSize, type);
    }

    private static int findBox(List<Box> boxes, String type) {
        for (int index = 0; index < boxes.size(); index++)
            if (type.equals(boxes.get(index).type)) return index;
        return -1;
    }

    private static byte[] readBox(File file, Box box) throws IOException {
        int size = checkedInt(box.size, "MP4 box too large");
        byte[] bytes = new byte[size];
        try (RandomAccessFile input = new RandomAccessFile(file, "r")) {
            input.seek(box.offset);
            input.readFully(bytes);
        }
        return bytes;
    }

    private static void copyBox(RandomAccessFile input, OutputStream output, Box box)
            throws IOException {
        input.seek(box.offset);
        copyBytes(input, output, box.size);
    }

    private static void copyFile(File source, File destination) throws IOException {
        try (BufferedInputStream input = new BufferedInputStream(new FileInputStream(source));
             BufferedOutputStream output = new BufferedOutputStream(
                     new FileOutputStream(destination))) {
            byte[] buffer = new byte[COPY_BUFFER_SIZE];
            int read;
            while ((read = input.read(buffer)) >= 0) {
                if (read > 0) output.write(buffer, 0, read);
            }
        }
    }

    private static void copyBytes(RandomAccessFile input, OutputStream output, long length)
            throws IOException {
        byte[] buffer = new byte[COPY_BUFFER_SIZE];
        long remaining = length;
        while (remaining > 0) {
            int requested = (int) Math.min(buffer.length, remaining);
            int read = input.read(buffer, 0, requested);
            if (read < 0) throw new IOException("Truncated MP4 box");
            output.write(buffer, 0, read);
            remaining -= read;
        }
    }

    private static int checkedInt(long value, String message) throws IOException {
        if (value < 0 || value > Integer.MAX_VALUE) throw new IOException(message);
        return (int) value;
    }

    private static long readUInt32(RandomAccessFile input) throws IOException {
        return ((long) input.readUnsignedByte() << 24)
                | ((long) input.readUnsignedByte() << 16)
                | ((long) input.readUnsignedByte() << 8)
                | input.readUnsignedByte();
    }

    private static long readUInt32(byte[] data, int offset) {
        return ((long) (data[offset] & 0xff) << 24)
                | ((long) (data[offset + 1] & 0xff) << 16)
                | ((long) (data[offset + 2] & 0xff) << 8)
                | (data[offset + 3] & 0xffL);
    }

    private static long readUInt64(byte[] data, int offset) throws IOException {
        long value = 0;
        for (int index = 0; index < 8; index++) {
            value = (value << 8) | (data[offset + index] & 0xffL);
        }
        if (value < 0) throw new IOException("MP4 box exceeds signed size range");
        return value;
    }

    private static void writeUInt(byte[] data, int offset, long value, int width)
            throws IOException {
        if (width == 4 && (value < 0 || value > UINT32_MAX))
            throw new IOException("MP4 32-bit offset overflow");
        if (width != 4 && width != 8) throw new IOException("Invalid MP4 integer width");
        if (width == 8 && value < 0) throw new IOException("MP4 64-bit offset overflow");
        for (int index = width - 1; index >= 0; index--) {
            data[offset + index] = (byte) (value & 0xff);
            value >>>= 8;
        }
    }

    private static void writeUInt64(byte[] data, int offset, long value) throws IOException {
        writeUInt(data, offset, value, 8);
    }

    private static final class Box {
        final long offset;
        final long size;
        final int headerSize;
        final String type;

        Box(long offset, long size, int headerSize, String type) {
            this.offset = offset;
            this.size = size;
            this.headerSize = headerSize;
            this.type = type;
        }
    }

    private static final class BoxHeader {
        final long size;
        final int headerSize;
        final int payloadStart;
        final String type;

        BoxHeader(long size, int headerSize, int payloadStart, String type) {
            this.size = size;
            this.headerSize = headerSize;
            this.payloadStart = payloadStart;
            this.type = type;
        }
    }
}
