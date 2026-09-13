package ai.links.app;

import android.content.Context;
import androidx.work.BackoffPolicy;
import androidx.work.Constraints;
import androidx.work.Data;
import androidx.work.ExistingWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.OneTimeWorkRequest;
import androidx.work.WorkManager;
import java.util.concurrent.TimeUnit;

/** Coalesces push hints into one durable network recovery job. */
public final class FcmRecoveryScheduler {
    static final String WORK_NAME = "links-fcm-mailbox-recovery";
    static final String INPUT_DEVICE_ID = "recipient_device_id";
    static final String INPUT_CURSOR = "cursor";
    static final String INPUT_FULL_SYNC = "full_sync";

    private FcmRecoveryScheduler() {}

    public static void enqueue(Context context, FcmWakeup wakeup) {
        if (context == null || wakeup == null) return;
        Data input = new Data.Builder()
                .putString(INPUT_DEVICE_ID, wakeup.recipientDeviceId())
                .putLong(INPUT_CURSOR, wakeup.cursor())
                .putBoolean(INPUT_FULL_SYNC, wakeup.fullSync())
                .build();
        Constraints constraints = new Constraints.Builder()
                .setRequiredNetworkType(NetworkType.CONNECTED)
                .build();
        OneTimeWorkRequest work = new OneTimeWorkRequest.Builder(FcmRecoveryWorker.class)
                .setConstraints(constraints)
                .setInputData(input)
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 10, TimeUnit.SECONDS)
                .build();
        WorkManager.getInstance(context.getApplicationContext())
                .enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.KEEP, work);
    }
}
