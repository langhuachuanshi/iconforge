import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useWorkspaceStore = defineStore('workspace', () => {
  const currentImage = ref('')
  const currentIconId = ref('')
  /** 当前图的来源标签（用于导出文件名）。编辑操作不清空它，只在新载入图时更新。 */
  const currentConcept = ref('')

  // 撤销/重做
  const undoStack = ref<string[]>([])
  const redoStack = ref<string[]>([])
  const isDirty = ref(false)

  function setImage(base64: string, iconId = '', concept = '') {
    currentImage.value = base64
    currentIconId.value = iconId
    currentConcept.value = concept
  }

  function clear() {
    currentImage.value = ''
    currentIconId.value = ''
    currentConcept.value = ''
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

  return { currentImage, currentIconId, currentConcept, undoStack, redoStack, isDirty, setImage, clear, pushHistory, undo, redo }
})
