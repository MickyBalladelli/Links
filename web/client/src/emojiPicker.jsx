const emojiGroups = {
  Faces: ['😀', '😃', '😄', '😁', '😆', '😅', '😂', '🤣', '😊', '😇', '🙂', '🙃', '😉', '😍', '😘', '🥰', '😋', '😎', '🥳', '🤩', '🤔', '🤗', '😴', '😭', '😡', '🤯', '🥺', '🫠'],
  Hands: ['👋', '🤚', '🖐️', '✋', '👌', '🤌', '🤏', '✌️', '🤞', '🫶', '🤟', '🤘', '🤙', '👈', '👉', '👆', '👇', '☝️', '👍', '👎', '✊', '👊', '👏', '🙌', '🫂', '🙏'],
  Hearts: ['❤️', '🩷', '🧡', '💛', '💚', '🩵', '💙', '💜', '🖤', '🩶', '🤍', '🤎', '💔', '❣️', '💕', '💞', '💓', '💗', '💖', '💘', '💝', '🔥', '💯', '✨'],
  Animals: ['🐶', '🐱', '🐭', '🐹', '🐰', '🦊', '🐻', '🐼', '🐨', '🐯', '🦁', '🐮', '🐷', '🐸', '🐵', '🐔', '🐧', '🦄', '🐝', '🦋', '🐢', '🐬', '🦈', '🐙'],
  Food: ['🍏', '🍎', '🍐', '🍊', '🍋', '🍌', '🍉', '🍇', '🍓', '🫐', '🍒', '🥭', '🍍', '🥑', '🥕', '🌽', '🍕', '🍔', '🍟', '🌭', '🌮', '🍣', '🍜', '🍰', '🍪', '☕', '🍺', '🍷'],
  Travel: ['🚗', '🚕', '🚌', '🚎', '🏎️', '🚲', '🛴', '✈️', '🛫', '🚀', '🛸', '🚁', '⛵', '🚢', '🏖️', '🏝️', '🏔️', '🗻', '🗽', '🗼', '🏠', '🏨', '🌍', '🌙'],
  Fun: ['⚽', '🏀', '🏈', '⚾', '🎾', '🏐', '🏆', '🥇', '🎮', '🎲', '🎯', '🎨', '🎭', '🎬', '🎤', '🎧', '🎸', '🎹', '🎁', '🎈', '🎉', '🎊', '🎆', '🎇'],
  Things: ['📱', '💻', '⌚', '📷', '💡', '🔦', '🕯️', '💰', '💎', '🔑', '🔒', '🔓', '🔨', '🧰', '🧲', '🧪', '💊', '📚', '✏️', '📝', '📌', '📎', '✉️', '📦'],
  Signs: ['✅', '❌', '❗', '❓', '‼️', '⁉️', '⚠️', '🚫', '🔞', '♻️', '💤', '💢', '💬', '👀', '🆗', '🆕', '🆙', '🔜', '⭐', '🌟', '☀️', '🌈', '☂️', '🎵']
}

export function EmojiPicker({ onSelect }) {
  function chooseEmoji(event) {
    const emoji = event.currentTarget.dataset.emoji
    if (!emoji) return
    onSelect(emoji)
  }

  return (
    <details class="emoji-picker">
      <summary aria-label="Add emoji" title="Add emoji">☺</summary>
      <div class="emoji-picker-menu" role="group" aria-label="Emoji">
        {Object.entries(emojiGroups).map(([name, emojis]) => (
          <section class="emoji-picker-group" aria-label={name}>
            <strong>{name}</strong>
            <div>
              {emojis.map(emoji => (
                <button type="button" data-emoji={emoji} aria-label={`Add ${emoji}`} onClick={chooseEmoji}>
                  {emoji}
                </button>
              ))}
            </div>
          </section>
        ))}
      </div>
    </details>
  )
}
