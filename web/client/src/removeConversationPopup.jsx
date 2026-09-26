import { computed } from '@mickyballadelli/matrix'
import { Button, Popup } from '@mickyballadelli/prism'

export function RemoveConversationPopup({ open, pending, onCancel, onConfirm }) {
  return (
    <Popup
      class="links-dialog"
      open={open}
      title="Remove conversation?"
      ariaDescription="This removes the conversation and its local attachments from this browser."
      size="small"
      onClose={onCancel}
      footer={() => (
        <div class="popup-actions">
          <Button label="Cancel" variant="secondary" onClick={onCancel} />
          <Button label="Remove" variant="primary" onClick={onConfirm} />
        </div>
      )}
    >
      <p>{computed(() => {
        const conversation = pending.value
        return conversation
          ? `Remove ${conversation.title}? Local messages and attachments will be deleted from this browser.`
          : 'Local messages and attachments will be deleted from this browser.'
      })}</p>
    </Popup>
  )
}
