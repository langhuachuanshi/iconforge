import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useWorkspaceStore = defineStore('workspace', () => {
  const currentImage = ref('')
  const currentIconId = ref('')

  // 撤销/重做
  const undoStack = ref<string[]>([])
  const redoStack = ref<string[]>([])
  const isDirty = ref(false)

  function setImage(base64: string, iconId = '') {
    currentImage.value = base64
    currentIconId.value = iconId
  }

  function clear() {
    currentImage.value = ''
    currentIconId.value = ''
    undoStack.value = []
    redoStack.value = []
    isDirty.value = false
  }

  function pushHistory() {
    if (!currentImage.value) return
    undoStack.value.push(currentImage.value)
    if (undoStack.value.length > 50) undoStack.value.shift()
    redoStack.value = []
    isDirty.value = true
  }

  function undo() {
    if (!undoStack.value.length) return
    redoStack.value.push(currentImage.value)
    currentImage.value = undoStack.value.pop()!
    isDirty.value = true
  }

  function redo() {
    if (!redoStack.value.length) return
    undoStack.value.push(currentImage.value)
    currentImage.value = redoStack.value.pop()!
    isDirty.value = true
  }

  return { currentImage, currentIconId, undoStack, redoStack, isDirty, setImage, clear, pushHistory, undo, redo }
})
