const emojis = [
  '😀', '😂', '😍', '🥳', '😎', '😢', '😡', '👍',
  '👎', '👏', '🙏', '❤️', '🔥', '🎉', '✅', '👀'
]

export function EmojiPicker({ onSelect }) {
  function chooseEmoji(event) {
    const emoji = event.currentTarget.dataset.emoji
    if (!emoji) return
    onSelect(emoji)
    event.currentTarget.closest('details')?.removeAttribute('open')
  }

  return (
    <details class="emoji-picker">
      <summary aria-label="Add emoji" title="Add emoji">☺</summary>
      <div class="emoji-picker-menu" role="group" aria-label="Emoji">
        {emojis.map(emoji => (
          <button type="button" data-emoji={emoji} aria-label={`Add ${emoji}`} onClick={chooseEmoji}>
            {emoji}
          </button>
        ))}
      </div>
    </details>
  )
}
