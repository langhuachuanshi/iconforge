import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'

// ---------- 类型 ----------

export interface ProviderInfo {
  name: string
  displayName: string
  configKey: string
  supportedSizes: string[]
  configured: boolean
}

export interface Template {
  id: string
  name: string
  description: string
}

export interface GenerateParams {
  concept: string
  style: string
  size: string
  provider: string
  custom_prompt?: string
}

export interface GenerateResult {
  image: string // base64
  icon_id: string
}

export interface CropParams {
  image: string
  x: number
  y: number
  width: number
  height: number
}

export interface IconMeta {
  id: string
  created_at: string
  concept: string
  style: string
  provider: string
}

// ---------- Tauri invoke 命令 ----------

export async function getProviders(): Promise<ProviderInfo[]> {
  return invoke('get_providers')
}

export async function getTemplates(): Promise<Template[]> {
  return invoke('get_templates')
}

export async function generateIcon(params: GenerateParams): Promise<GenerateResult> {
  return invoke('generate_icon', { req: params })
}

export async function cropImage(params: CropParams): Promise<string> {
  const result = await invoke<{ image: string }>('crop_image', { req: params })
  return result.image
}

export async function removeBackground(image: string, threshold?: number): Promise<string> {
  try {
    const result = await invoke<{ image: string }>('remove_background', {
      req: { image, threshold: threshold ?? 0.5 }
    })
    return result.image
  } catch (e: any) {
    throw new Error(typeof e === 'string' ? e : e?.message || '抠图失败')
  }
}

/**
 * 导出图标：弹出保存对话框，Rust 后端直接写入用户选定的文件路径
 */
export async function exportIcon(
  image: string,
  pngSizes?: number[],
  icoSizes?: number[]
): Promise<void> {
  const filePath = await save({
    defaultPath: 'icon-export.zip',
    filters: [{ name: 'ZIP 压缩包', extensions: ['zip'] }],
  })
  if (!filePath) return // 用户取消

  await invoke('export_icon_to_file', {
    req: {
      image,
      pngSizes: pngSizes ?? null,
      icoSizes: icoSizes ?? null,
    },
    savePath: filePath,
  })
}

export async function listIcons(): Promise<IconMeta[]> {
  const result = await invoke<{ icons: IconMeta[] }>('list_icons')
  return result.icons
}

export async function deleteIcon(iconId: string): Promise<void> {
  await invoke('delete_icon', { iconId })
}

/** 从历史加载 base64（用于编辑页） */
export async function fetchIconBase64(iconId: string): Promise<string> {
  const result = await invoke<{ image: string }>('get_icon_base64', { iconId })
  return result.image
}

/** 获取图标文件路径，用于 convertFileSrc */
export async function getIconPath(iconId: string): Promise<string> {
  return invoke('get_icon_path', { iconId })
}

/** 读取所有配置 */
export async function getConfig(): Promise<Record<string, string>> {
  return invoke('get_config')
}

/** 设置配置 */
export async function setConfig(key: string, value: string): Promise<void> {
  await invoke('set_config', { key, value })
}

// ---------- 服务商管理 ----------

export interface ProviderEntry {
  id: string
  name: string
  notes: string
  website: string
  apiKey: string
  endpoint: string
  model: string
  isBuiltin: boolean
  enabled: boolean
}

export interface ProviderUpsertRequest {
  id?: string
  name: string
  notes?: string
  website?: string
  apiKey: string
  endpoint: string
  model?: string
}

export async function listProviders(): Promise<ProviderEntry[]> {
  return invoke('list_providers')
}

export async function addProvider(req: ProviderUpsertRequest): Promise<ProviderEntry> {
  return invoke('add_provider', { req })
}

export async function updateProvider(id: string, req: ProviderUpsertRequest): Promise<void> {
  await invoke('update_provider', { id, req })
}

export async function deleteProvider(id: string): Promise<void> {
  await invoke('delete_provider', { id })
}

export async function toggleProvider(id: string, enabled: boolean): Promise<void> {
  await invoke('toggle_provider', { id, enabled })
}

export async function reorderProviders(ids: string[]): Promise<void> {
  await invoke('reorder_providers', { ids })
}

// ---------- 抠图模型 ----------

export async function checkBgModel(): Promise<{ downloaded: boolean; model: string }> {
  return invoke('check_bg_model')
}

export async function downloadBgModel(
  onProgress?: (pct: number) => void
): Promise<void> {
  const { listen } = await import('@tauri-apps/api/event')

  const unlisten = await listen<{ percent: number }>(
    'bg-download-progress',
    (event) => {
      onProgress?.(event.payload.percent)
    }
  )

  try {
    await invoke('download_bg_model')
  } finally {
    unlisten()
  }
}

// ---------- 工具函数 ----------

/** File/Blob 转 base64（不含 data: 前缀） */
export function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = reader.result as string
      resolve(result.split(',')[1])
    }
    reader.onerror = reject
    reader.readAsDataURL(blob)
  })
}

/** base64 字符串加上 data URL 前缀，用于 <img :src> */
export function toDataUrl(b64: string, format = 'image/png'): string {
  if (b64.startsWith('data:')) return b64
  return `data:${format};base64,${b64}`
}
