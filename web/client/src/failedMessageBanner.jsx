export function FailedMessageBanner({ error, sending, onRetry }) {
  return (
    <div class="message-send-banner" role="alert">
      <span>{sending ? 'Sending…' : `Could not send: ${error}`}</span>
      <button type="button" disabled={sending} onClick={onRetry}>
        {sending ? 'Sending…' : 'Send again'}
      </button>
    </div>
  )
}
