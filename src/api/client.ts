import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'

// ---------- 内部 helper ----------

/**
 * 「弹保存对话框 → 调后端命令写文件」的统一流程。
 * 返回 true 表示已写入，false 表示用户取消。
 */
async function saveAndInvoke(
  cmd: string,
  defaultName: string,
  filterName: string,
  extensions: string[],
  buildArgs: (savePath: string) => Record<string, unknown>,
): Promise<boolean> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: filterName, extensions }],
  })
  if (!filePath) return false // 用户取消
  await invoke(cmd, buildArgs(filePath))
  return true
}

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

export async function removeBackground(image: string): Promise<string> {
  try {
    const result = await invoke<{ image: string }>('remove_background', {
      req: { image, threshold: 0.0 }
    })
    return result.image
  } catch (e: any) {
    throw new Error(typeof e === 'string' ? e : e?.message || '抠图失败')
  }
}

/** 按颜色去底（魔棒/色键）。color 为 [r,g,b]，tolerance 0~442 */
export async function removeColor(
  image: string,
  color: [number, number, number],
  tolerance: number,
): Promise<string> {
  try {
    const result = await invoke<{ image: string }>('remove_color', {
      req: { image, color, tolerance },
    })
    return result.image
  } catch (e: any) {
    throw new Error(typeof e === 'string' ? e : e?.message || '去底色失败')
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
  await saveAndInvoke(
    'export_icon_to_file',
    'icon-export.zip',
    'ZIP 压缩包',
    ['zip'],
    (savePath) => ({
      req: {
        image,
        pngSizes: pngSizes ?? null,
        icoSizes: icoSizes ?? null,
      },
      savePath,
    }),
  )
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

export interface BgModelEntry {
  id: string
  name: string
  size: string
  downloaded: boolean
  /** 已下载时的完整文件路径，未下载为 null */
  path: string | null
  /** 是否为当前选中模型 */
  current: boolean
}

export async function checkBgModel(): Promise<{ downloaded: boolean; model: string }> {
  return invoke('check_bg_model')
}

/** 列出所有抠图模型及其下载状态 */
export async function listBgModels(): Promise<BgModelEntry[]> {
  return invoke('list_bg_models')
}

/** 删除已下载的模型文件 */
export async function deleteBgModel(modelId: string): Promise<void> {
  await invoke('delete_bg_model', { modelId })
}

/** 在系统资源管理器中打开模型所在位置 */
export async function openModelLocation(modelId: string): Promise<void> {
  await invoke('open_model_location', { modelId })
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

// ---------- 图标提取（PE → ICO）----------

export interface ExtractedIcon {
  /** 所属图标组名（如 MAINICON），同组的条目共享 */
  name: string
  width: number
  height: number
  bitDepth: number
  /** 该尺寸的 PNG base64（前端用 <img> 直接显示） */
  pngBase64: string
  /** 整组的 ICO base64（同组共享，用于「导出整组为 ICO」） */
  icoBase64: string
}

/** 从 PE 文件提取所有图标 */
export async function extractIcons(filePath: string): Promise<ExtractedIcon[]> {
  return invoke('extract_icons', { req: { filePath } })
}

/** 保存单个 ICO 到磁盘（复用后端 save_image_file，它就是写字节） */
export async function saveIco(icoBase64: string, defaultName = 'icon.ico'): Promise<void> {
  await saveAndInvoke(
    'save_image_file',
    defaultName,
    'ICO 图标',
    ['ico'],
    (savePath) => ({ savePath, image: icoBase64 }),
  )
}

/** 保存单个 PNG 到磁盘，返回是否实际写入（false = 用户取消） */
export async function savePng(pngBase64: string, defaultName = 'icon.png'): Promise<boolean> {
  return saveAndInvoke(
    'save_image_file',
    defaultName,
    'PNG 图片',
    ['png'],
    (savePath) => ({ savePath, image: pngBase64 }),
  )
}

// ---------- 多图转 ICO ----------

/** 多张图片转单个 ICO 并保存 */
export async function convertImagesToIco(images: string[], sizes: number[]): Promise<void> {
  await saveAndInvoke(
    'convert_images_to_ico',
    'icons.ico',
    'ICO 图标',
    ['ico'],
    (savePath) => ({ req: { images, sizes }, savePath }),
  )
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
