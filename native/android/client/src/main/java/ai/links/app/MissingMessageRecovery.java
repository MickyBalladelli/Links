package ai.links.app;

/** Host hook that runs the shared Rust receive coordinator over Android TLS. */
public interface MissingMessageRecovery {
    enum Outcome {
        COMPLETE,
        RETRY,
        AUTHENTICATION_REQUIRED
    }

    /** The cursor hint is not authoritative; read the durable cursor locally. */
    Outcome recover(long cursorHint, boolean fullSync) throws Exception;
}
