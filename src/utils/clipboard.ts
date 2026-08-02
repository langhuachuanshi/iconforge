import { ElMessage } from 'element-plus'

/**
 * 复制文本到剪贴板，成功后弹出提示。
 *
 * 优先用 navigator.clipboard（Tauri webview 支持）；
 * 失败（如非安全上下文 / API 不可用）回退到 execCommand('copy')。
 *
 * @param text 待复制文本
 * @param successMsg 成功提示文案
 * @returns 是否复制成功
 */
export async function copyText(text: string, successMsg = '已复制'): Promise<boolean> {
  if (!text) {
    ElMessage.warning('内容为空')
    return false
  }
  // 优先：异步 Clipboard API
  try {
    await navigator.clipboard.writeText(text)
    ElMessage.success(successMsg)
    return true
  } catch {
    // 兜底：execCommand（已废弃但仍广泛可用，覆盖非安全上下文）
    try {
      const ta = document.createElement('textarea')
      ta.value = text
      ta.style.position = 'fixed'
      ta.style.opacity = '0'
      document.body.appendChild(ta)
      ta.select()
      const ok = document.execCommand('copy')
      document.body.removeChild(ta)
      if (ok) {
        ElMessage.success(successMsg)
        return true
      }
    } catch {
      /* 忽略，落到下方错误提示 */
    }
    ElMessage.error('复制失败')
    return false
  }
}
