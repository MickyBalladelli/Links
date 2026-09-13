package ai.links.app;

import java.io.IOException;
import java.util.Map;
import java.util.UUID;

/** Strict parser for the gateway's data-only FCM wakeup. */
public final class FcmWakeup {
    public static final String RECIPIENT_DEVICE_ID = "recipient_device_id";
    public static final String CURSOR = "cursor";

    private final String recipientDeviceId;
    private final long cursor;
    private final boolean fullSync;

    private FcmWakeup(String recipientDeviceId, long cursor, boolean fullSync) {
        this.recipientDeviceId = recipientDeviceId;
        this.cursor = cursor;
        this.fullSync = fullSync;
    }

    public static FcmWakeup parse(Map<String, String> data) throws IOException {
        if (data == null || data.size() != 2 || !data.containsKey(RECIPIENT_DEVICE_ID)
                || !data.containsKey(CURSOR))
            throw new IOException("Invalid FCM wakeup");
        String deviceId = data.get(RECIPIENT_DEVICE_ID);
        String cursorValue = data.get(CURSOR);
        canonicalUuid(deviceId);
        long cursor = parseCursor(cursorValue);
        return new FcmWakeup(deviceId, cursor, false);
    }

    public static FcmWakeup fullSync(String recipientDeviceId) throws IOException {
        canonicalUuid(recipientDeviceId);
        return new FcmWakeup(recipientDeviceId, 0, true);
    }

    public String recipientDeviceId() {
        return recipientDeviceId;
    }

    public long cursor() {
        return cursor;
    }

    public boolean fullSync() {
        return fullSync;
    }

    static void canonicalUuid(String value) throws IOException {
        if (value == null) throw new IOException("Invalid FCM device ID");
        try {
            UUID parsed = UUID.fromString(value);
            if (parsed.getMostSignificantBits() == 0 && parsed.getLeastSignificantBits() == 0
                    || !parsed.toString().equals(value))
                throw new IllegalArgumentException();
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid FCM device ID", error);
        }
    }

    static long parseCursor(String value) throws IOException {
        if (value == null || value.isEmpty()) throw new IOException("Invalid FCM cursor");
        try {
            long cursor = Long.parseLong(value);
            if (cursor <= 0 || !Long.toString(cursor).equals(value))
                throw new IllegalArgumentException();
            return cursor;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid FCM cursor", error);
        }
    }
}
