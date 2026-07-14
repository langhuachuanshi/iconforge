import { defineStore } from 'pinia'
import { ref } from 'vue'

/**
 * 工作区状态：当前正在处理的图片。
 * 用 store 而非组件局部 ref，保证切换路由时图片不丢失。
 */
export const useWorkspaceStore = defineStore('workspace', () => {
  // 当前图片的 base64（空字符串表示无图片）
  const currentImage = ref('')
  // 当前图片对应的 icon_id（用于关联历史记录，可能为空表示未保存）
  const currentIconId = ref('')

  function setImage(base64: string, iconId = '') {
    currentImage.value = base64
    currentIconId.value = iconId
  }

  function clear() {
    currentImage.value = ''
    currentIconId.value = ''
  }

  return { currentImage, currentIconId, setImage, clear }
})
