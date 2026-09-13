package ai.links.app;

import android.content.Context;
import androidx.annotation.NonNull;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

/** Runs mailbox replay outside FCM's short onMessageReceived callback window. */
public final class FcmRecoveryWorker extends Worker {
    public FcmRecoveryWorker(@NonNull Context context, @NonNull WorkerParameters parameters) {
        super(context, parameters);
    }

    @NonNull
    @Override
    public Result doWork() {
        String deviceId = getInputData().getString(FcmRecoveryScheduler.INPUT_DEVICE_ID);
        long cursorHint = getInputData().getLong(FcmRecoveryScheduler.INPUT_CURSOR, 0);
        boolean fullSync = getInputData().getBoolean(FcmRecoveryScheduler.INPUT_FULL_SYNC, false);
        if (!validInput(deviceId, cursorHint, fullSync)) return Result.failure();

        try {
            ClientSession session = new ClientSession(getApplicationContext());
            if (!deviceId.equals(session.deviceId())) return Result.failure();
            LinksApplication app = (LinksApplication) getApplicationContext();
            MissingMessageRecovery recovery = app.createMissingMessageRecovery();
            if (recovery == null) return Result.failure();
            switch (recovery.recover(cursorHint, fullSync)) {
                case COMPLETE:
                    return Result.success();
                case AUTHENTICATION_REQUIRED:
                    return Result.failure();
                case RETRY:
                default:
                    return Result.retry();
            }
        } catch (Exception error) {
            return Result.retry();
        }
    }

    private static boolean validInput(String deviceId, long cursorHint, boolean fullSync) {
        try {
            FcmWakeup.canonicalUuid(deviceId);
        } catch (Exception error) {
            return false;
        }
        return fullSync ? cursorHint == 0 : cursorHint > 0;
    }
}
