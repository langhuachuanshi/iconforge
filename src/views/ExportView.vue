<script setup lang="ts">
import { computed, onActivated, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { blobToBase64, toDataUrl } from '../api/client'
import { useWorkspaceStore } from '../stores/workspace'

interface ExportImage {
  /** base64（不含 data: 前缀），发给后端 */
  b64: string
  /** data url，用于 <img> 预览 */
  dataUrl: string
  name: string
  /** 是否来自编辑区（编辑结果 vs 本地上传） */
  fromEditor: boolean
}

const workspace = useWorkspaceStore()

const images = ref<ExportImage[]>([])
const processing = ref(false)
const pngSizes = ref<number[]>([16, 32, 48, 64, 128, 256, 512])
const icoSizes = ref<number[]>([16, 32, 48, 64, 128, 256])
const pngAll = [16, 32, 48, 64, 128, 256, 512]
const icoAll = [16, 32, 48, 64, 128, 256]

// 导出进度
const exportTotal = ref(0)
const exportCurrent = ref(0)

// ── 尺寸预设 ──
type PresetKey = 'all' | 'ico' | 'png512' | 'custom'
const activePreset = ref<PresetKey>('all')
const presets: { key: PresetKey; label: string; png: number[]; ico: number[] }[] = [
  { key: 'all', label: '全尺寸', png: pngAll, ico: icoAll },
  { key: 'ico', label: '标准 ICO', png: [], ico: icoAll },
  { key: 'png512', label: '高清 PNG', png: [512], ico: [] },
  { key: 'custom', label: '自定义', png: [], ico: [] },
]
function applyPreset(key: PresetKey) {
  activePreset.value = key
  const p = presets.find((x) => x.key === key)
  if (p && key !== 'custom') {
    pngSizes.value = [...p.png]
    icoSizes.value = [...p.ico]
  }
}
// 手动改尺寸 → 切回自定义
function onSizeManualChange() {
  activePreset.value = 'custom'
}

// keep-alive 缓存后用 onActivated：每次切回导出页，若编辑区有新结果则补入列表。
onActivated(() => {
  const cur = workspace.currentImage
  if (cur) {
    const exists = images.value.some((i) => i.fromEditor)
    if (!exists) {
      images.value.unshift({
        b64: cur,
        dataUrl: toDataUrl(cur),
        name: '编辑结果',
        fromEditor: true,
      })
    } else {
      // 编辑结果已存在，刷新为最新（编辑可能又改了）
      images.value = images.value.map((i) =>
        i.fromEditor
          ? { ...i, b64: cur, dataUrl: toDataUrl(cur) }
          : i
      )
    }
  }
})

/** el-upload before-upload：拦截 File，转 base64 加入列表 */
async function onFilePicked(file: File) {
  try {
    const b64 = await blobToBase64(file)
    images.value.push({ b64, dataUrl: toDataUrl(b64), name: file.name, fromEditor: false })
  } catch (e: any) {
    ElMessage.error('读取失败：' + (e?.message || e))
  }
  return false // 阻止默认上传
}

// ── 拖拽添加图片 ──
const dragOver = ref(false)
function onDragEnter(e: DragEvent) {
  if (!e.dataTransfer?.types.includes('Files')) return
  e.preventDefault()
  dragOver.value = true
}
function onDragOver(e: DragEvent) {
  if (!e.dataTransfer?.types.includes('Files')) return
  e.preventDefault()
  e.dataTransfer.dropEffect = 'copy'
}
function onDragLeave(e: DragEvent) {
  if (e.relatedTarget === null) dragOver.value = false
}
async function onDrop(e: DragEvent) {
  e.preventDefault()
  dragOver.value = false
  const files = Array.from(e.dataTransfer?.files ?? []).filter((f) => f.type.startsWith('image/'))
  if (files.length === 0) return
  for (const f of files) await onFilePicked(f)
  ElMessage.success(`已添加 ${files.length} 张图片`)
}

function removeImage(idx: number) {
  images.value.splice(idx, 1)
}
function clearAll() {
  images.value = []
}

// 大图预览
const previewSrc = ref('')
const previewVisible = computed({
  get: () => !!previewSrc.value,
  set: (v: boolean) => { if (!v) previewSrc.value = '' },
})

const totalSelectedSizes = computed(() => pngSizes.value.length + icoSizes.value.length)
const exportBtnText = computed(() => {
  if (!processing.value) return `导出 ${images.value.length} 张到文件夹`
  if (exportTotal.value > 1) return `导出中 ${exportCurrent.value}/${exportTotal.value}...`
  return '导出中...'
})

async function handleExport() {
  if (images.value.length === 0) {
    ElMessage.warning('请先添加图片')
    return
  }
  if (totalSelectedSizes.value === 0) {
    ElMessage.warning('请至少选择一个尺寸')
    return
  }

  // 选导出目录
  let dir: string | null = null
  try {
    const selected = await open({ directory: true, multiple: false })
    if (!selected || Array.isArray(selected)) {
      ElMessage.info('已取消')
      return
    }
    dir = selected as string
  } catch (e: any) {
    ElMessage.error('选择目录失败：' + (e?.message || e))
    return
  }

  processing.value = true
  exportTotal.value = images.value.length
  exportCurrent.value = 0
  let ok = 0
  let fail = 0

  try {
    for (let i = 0; i < images.value.length; i++) {
      exportCurrent.value = i + 1
      const img = images.value[i]
      // 文件名：原名(去扩展名) + 序号避免重名
      const baseName = img.name.replace(/\.[^.]+$/, '') || `icon_${i + 1}`
      const savePath = `${dir}/${baseName}_${i + 1}.zip`
      try {
        await invoke('export_icon_to_file', {
          req: {
            image: img.b64,
            pngSizes: pngSizes.value,
            icoSizes: icoSizes.value,
          },
          savePath,
        })
        ok++
      } catch (e: any) {
        fail++
        console.error(`导出 ${img.name} 失败:`, e?.message || e)
      }
    }
    if (ok > 0) {
      ElMessage.success(`已导出 ${ok} 个文件到所选目录${fail > 0 ? `（${fail} 个失败）` : ''}`)
    } else if (fail > 0) {
      ElMessage.error(`导出失败 ${fail} 个`)
    }
  } finally {
    processing.value = false
    exportTotal.value = 0
    exportCurrent.value = 0
  }
}
</script>

<template>
  <div
    class="export-root"
    :class="{ 'drag-active': dragOver && images.length > 0 }"
    @dragenter="onDragEnter"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <!-- 顶部栏（有图才显示） -->
    <div class="top-bar" v-if="images.length > 0">
      <div class="top-left">
        <el-upload
          :show-file-list="false"
          :before-upload="onFilePicked"
          accept="image/png,image/jpeg,image/bmp,image/webp"
          multiple
        >
          <el-button size="small"><el-icon><Plus /></el-icon> 添加图片</el-button>
        </el-upload>
      </div>
      <div class="top-right">
        <el-button v-if="images.length" size="small" text @click="clearAll">清空</el-button>
      </div>
    </div>
    <p class="header-hint" v-if="images.length > 0">共 {{ images.length }} 张，拖拽可继续添加</p>

    <!-- 空状态：居中大入口（export-root 直接子元素，与编辑/提取页统一） -->
    <div v-if="images.length === 0" class="empty-hero">
      <el-upload
        :show-file-list="false"
        :before-upload="onFilePicked"
        accept="image/png,image/jpeg,image/bmp,image/webp"
        multiple
        class="hero-upload"
      >
        <div class="hero-card" :class="{ 'drag-hover': dragOver }">
          <el-icon :size="64" class="hero-icon"><UploadFilled /></el-icon>
          <div class="hero-title">添加图片</div>
          <div class="hero-hint">点击选择，或拖拽图片到此处</div>
        </div>
      </el-upload>
    </div>

    <!-- 图片列表（有图才显示） -->
    <div v-if="images.length > 0" class="list-area" v-loading="processing" :element-loading-text="exportTotal > 1 ? `导出中 ${exportCurrent}/${exportTotal}...` : undefined">
      <div class="thumb-list">
        <div v-for="(img, idx) in images" :key="idx" class="thumb-item">
          <img :src="img.dataUrl" :alt="img.name" @click="previewSrc = img.dataUrl" />
          <div class="thumb-info">
            <span class="thumb-name" :title="img.name">
              <span class="thumb-idx">{{ idx + 1 }}</span>
              {{ img.name }}
              <el-tag v-if="img.fromEditor" size="small" type="success">编辑结果</el-tag>
            </span>
            <el-button text size="small" type="danger" @click="removeImage(idx)">
              <el-icon><Delete /></el-icon>
            </el-button>
          </div>
        </div>
      </div>
    </div>

    <!-- 底部：尺寸预设 + 详细尺寸 + 导出（有图才显示） -->
    <el-card v-if="images.length > 0" class="options-card" shadow="never">
      <!-- 尺寸预设快捷 -->
      <div class="preset-row">
        <span class="preset-label">尺寸方案：</span>
        <el-radio-group v-model="activePreset" size="small" @change="(v: any) => applyPreset(v as PresetKey)">
          <el-radio-button v-for="p in presets" :key="p.key" :value="p.key">{{ p.label }}</el-radio-button>
        </el-radio-group>
      </div>

      <el-form label-position="top" size="small">
        <el-form-item label="PNG 尺寸">
          <el-checkbox-group v-model="pngSizes" @change="onSizeManualChange">
            <el-checkbox v-for="s in pngAll" :key="s" :value="s">{{ s }}</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-form-item label="ICO 尺寸">
          <el-checkbox-group v-model="icoSizes" @change="onSizeManualChange">
            <el-checkbox v-for="s in icoAll" :key="s" :value="s">{{ s }}</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-button
          type="primary"
          :loading="processing"
          :disabled="images.length === 0 || totalSelectedSizes === 0"
          @click="handleExport"
          size="large"
          style="width: 100%"
        >
          <el-icon><Download /></el-icon>&nbsp;{{ exportBtnText }}（每张 × {{ totalSelectedSizes }} 尺寸，各自一个 ZIP）
        </el-button>
      </el-form>
    </el-card>

    <!-- 大图预览 -->
    <el-dialog v-model="previewVisible" width="auto" :show-close="true" append-to-body>
      <img :src="previewSrc" class="preview-large" alt="预览" />
    </el-dialog>

    <!-- 拖拽遮罩 -->
    <div v-if="dragOver && images.length > 0" class="drop-overlay">
      <el-icon :size="48"><UploadFilled /></el-icon>
      <p>松开以添加图片</p>
    </div>
  </div>
</template>

<style scoped>
.export-root { display: flex; flex-direction: column; height: calc(100vh - 110px); position: relative; }

.top-bar {
  display: flex; align-items: center; margin-bottom: 8px; flex-shrink: 0; gap: 8px;
}
.top-left { display: flex; gap: 4px; flex: 1; }
.top-right { display: flex; gap: 4px; align-items: center; }
.header-hint { color: var(--el-text-color-secondary); font-size: 13px; margin: 0 0 12px; }

.list-area { flex: 1; overflow-y: auto; min-height: 200px; }

.empty-icon { color: var(--el-text-color-placeholder); }

/* 空状态居中大入口（与编辑/提取页统一风格） */
.empty-hero { flex: 1; display: flex; align-items: center; justify-content: center; }
.hero-upload { display: block; }
.hero-card {
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  width: 360px; padding: 56px 32px;
  border: 2px dashed var(--el-border-color); border-radius: 16px;
  background: var(--el-fill-color-light); cursor: pointer;
  transition: all 0.2s;
}
.hero-card:hover, .hero-card.drag-hover {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
  transform: translateY(-2px);
}
.hero-icon { color: var(--el-color-primary); margin-bottom: 20px; }
.hero-title { font-size: 20px; font-weight: 600; color: var(--el-text-color-primary); }
.hero-hint { font-size: 13px; color: var(--el-text-color-secondary); margin-top: 10px; }

.thumb-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 10px;
}
.thumb-item {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  overflow: hidden;
  background: var(--el-bg-color);
}
.thumb-item img {
  width: 100%;
  height: 140px;
  object-fit: contain;
  background: var(--el-fill-color-light);
  cursor: zoom-in;
}
.thumb-item img:hover { opacity: 0.85; }
.thumb-info {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 4px 8px;
}
.thumb-name {
  flex: 1;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.thumb-idx {
  display: inline-block;
  min-width: 16px;
  height: 16px;
  line-height: 16px;
  text-align: center;
  background: var(--el-color-primary-light-8);
  color: var(--el-color-primary);
  border-radius: 50%;
  font-size: 10px;
  flex-shrink: 0;
}

.options-card { flex-shrink: 0; }
.preset-row { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
.preset-label { font-size: 13px; color: var(--el-text-color-secondary); flex-shrink: 0; }

.preview-large { max-width: 80vw; max-height: 80vh; object-fit: contain; }

/* 拖拽高亮 */
.export-root.drag-active > *:not(.drop-overlay) { filter: brightness(0.6); }
.drop-overlay {
  position: absolute; inset: 0; z-index: 9999;
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  background: var(--el-color-primary-light-9); color: var(--el-color-primary);
  border: 3px dashed var(--el-color-primary); border-radius: 6px;
  pointer-events: none;
}
.drop-overlay p { margin-top: 12px; font-size: 18px; font-weight: 600; }
</style>
