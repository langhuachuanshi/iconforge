<script setup lang="ts">
import { ref, computed } from 'vue'
import { ElMessage } from 'element-plus'
import { extractIcons, extractIconsFromBytes, saveIco, savePng, blobToBase64, type ExtractedIcon } from '../api/client'

const icons = ref<ExtractedIcon[]>([])
const processing = ref(false)
const filePath = ref('')

const PE_EXTS = ['exe', 'dll', 'ocx', 'cpl']

/** 按组聚合：组名 → 该组所有尺寸条目（用于「导出整组 ICO」去重） */
const groups = computed(() => {
  const map = new Map<string, ExtractedIcon[]>()
  for (const item of icons.value) {
    if (!map.has(item.name)) map.set(item.name, [])
    map.get(item.name)!.push(item)
  }
  return Array.from(map.entries()) // [groupName, entries[]][]
})

function pngUrl(b64: string): string {
  return `data:image/png;base64,${b64}`
}

function pickFile() {
  return import('@tauri-apps/plugin-dialog').then(({ open }) =>
    open({
      multiple: false,
      filters: [{ name: 'PE 文件', extensions: ['exe', 'dll', 'ocx', 'cpl'] }],
    })
  )
}

async function handlePick() {
  try {
    const selected = await pickFile()
    if (!selected || Array.isArray(selected)) return
    filePath.value = selected as string
    await load()
  } catch (e: any) {
    ElMessage.error('选择文件失败：' + (e?.message || e))
  }
}

async function load() {
  if (!filePath.value) return
  processing.value = true
  try {
    icons.value = await extractIcons(filePath.value)
    if (icons.value.length === 0) {
      ElMessage.warning('该文件没有图标资源')
    } else {
      ElMessage.success(`提取出 ${icons.value.length} 个尺寸`)
    }
  } catch (e: any) {
    ElMessage.error('提取失败：' + (e?.message || e))
    icons.value = []
  } finally {
    processing.value = false
  }
}

// ── 拖拽提取（按字节）──
const dragOver = ref(false)

function isPeFile(file: File): boolean {
  const ext = file.name.split('.').pop()?.toLowerCase() ?? ''
  return PE_EXTS.includes(ext)
}

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
  const files = Array.from(e.dataTransfer?.files ?? [])
  const pe = files.find(isPeFile)
  if (!pe) {
    ElMessage.warning('请拖入 .exe / .dll / .ocx 文件')
    return
  }
  processing.value = true
  try {
    const b64 = await blobToBase64(pe)
    icons.value = await extractIconsFromBytes(b64)
    filePath.value = pe.name
    if (icons.value.length === 0) {
      ElMessage.warning('该文件没有图标资源')
    } else {
      ElMessage.success(`提取出 ${icons.value.length} 个尺寸`)
    }
  } catch (err: any) {
    ElMessage.error('提取失败：' + (err?.message || err))
    icons.value = []
  } finally {
    processing.value = false
  }
}

async function handleExportPng(icon: ExtractedIcon) {
  if (!icon.pngBase64) {
    ElMessage.warning('该尺寸无 PNG 预览')
    return
  }
  try {
    await savePng(icon.pngBase64, `${icon.name}_${icon.width}x${icon.height}.png`)
    ElMessage.success('已导出 PNG')
  } catch (e: any) {
    ElMessage.error('导出失败：' + (e?.message || e))
  }
}

async function handleExportGroupIco(groupName: string, sample: ExtractedIcon) {
  try {
    await saveIco(sample.icoBase64, `${groupName}.ico`)
    ElMessage.success('已导出 ICO')
  } catch (e: any) {
    ElMessage.error('导出失败：' + (e?.message || e))
  }
}
</script>

<template>
  <div
    class="extract-root"
    :class="{ 'drag-active': dragOver && icons.length > 0 }"
    @dragenter="onDragEnter"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <!-- 顶部栏（有结果才显示） -->
    <div class="top-bar" v-if="icons.length > 0">
      <div class="top-left">
        <el-button size="small" @click="handlePick" :loading="processing">
          <el-icon><FolderOpened /></el-icon> 选择 PE 文件
        </el-button>
      </div>
      <div class="top-right">
        <el-button v-if="filePath" size="small" text @click="load" :loading="processing">重新提取</el-button>
      </div>
    </div>
    <p v-if="filePath && icons.length > 0" class="header-hint file-path" :title="filePath">{{ filePath }}</p>

    <!-- 空状态：居中大入口 -->
    <div v-if="!processing && icons.length === 0" class="empty-hero">
      <div class="hero-card" :class="{ 'drag-hover': dragOver }" @click="handlePick">
        <el-icon :size="64" class="hero-icon"><FolderOpened /></el-icon>
        <div class="hero-title">提取图标</div>
        <div class="hero-hint">选择 .exe / .dll / .ocx 文件，或拖拽到此处</div>
      </div>
    </div>

    <!-- 提取结果（有结果或加载中才显示，否则让 hero 居中占满） -->
    <div v-if="icons.length > 0 || processing" v-loading="processing" class="result-area">
      <el-empty v-if="processing && icons.length === 0" description="" />

      <!-- 按组分区展示 -->
      <div v-else class="groups">
        <div v-for="[groupName, entries] in groups" :key="groupName" class="group">
          <div class="group-header">
            <span class="group-name">{{ groupName }}</span>
            <el-button
              size="small"
              type="primary"
              plain
              @click="handleExportGroupIco(groupName, entries[0])"
              title="导出整组为 .ico"
            >
              <el-icon><Download /></el-icon>&nbsp;导出整组 ICO
            </el-button>
          </div>

          <div class="icon-grid">
            <el-card
              v-for="(icon, idx) in entries"
              :key="idx"
              class="icon-card"
              :body-style="{ padding: '0' }"
              shadow="hover"
            >
              <div class="icon-thumb">
                <img v-if="icon.pngBase64" :src="pngUrl(icon.pngBase64)" :alt="`${groupName} ${icon.width}`" />
                <el-icon v-else :size="32"><Picture /></el-icon>
              </div>
              <div class="icon-info">
                <div class="info-meta">
                  <span class="info-size">{{ icon.width }}×{{ icon.height }}</span>
                  <el-tag size="small" type="info">{{ icon.bitDepth }}bpp</el-tag>
                </div>
                <el-button size="small" type="primary" @click="handleExportPng(icon)">
                  <el-icon><Download /></el-icon>&nbsp;PNG
                </el-button>
              </div>
            </el-card>
          </div>
        </div>
      </div>
    </div>

    <!-- 拖拽遮罩 -->
    <div v-if="dragOver && icons.length > 0" class="drop-overlay">
      <el-icon :size="48"><UploadFilled /></el-icon>
      <p>松开以提取图标</p>
    </div>
  </div>
</template>

<style scoped>
.extract-root { display: flex; flex-direction: column; height: calc(100vh - 110px); position: relative; }

/* 拖拽高亮 */
.extract-root.drag-active > *:not(.drop-overlay) { filter: brightness(0.6); }
.drop-overlay {
  position: absolute; inset: 0; z-index: 9999;
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  background: var(--el-color-primary-light-9); color: var(--el-color-primary);
  border: 3px dashed var(--el-color-primary); border-radius: 6px;
  pointer-events: none;
}
.drop-overlay p { margin-top: 12px; font-size: 18px; font-weight: 600; }

.top-bar {
  display: flex; align-items: center; margin-bottom: 8px; flex-shrink: 0; gap: 8px;
}
.top-left { display: flex; gap: 4px; flex: 1; }
.top-right { display: flex; gap: 4px; align-items: center; }
.header-hint { margin: 0 0 12px; font-size: 13px; }

.file-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-text-color-secondary);
  font-family: ui-monospace, monospace;
}

.result-area { flex: 1; overflow-y: auto; }

/* 空状态居中大入口（与编辑页统一风格） */
.empty-hero { flex: 1; display: flex; align-items: center; justify-content: center; }
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
.hero-hint { font-size: 13px; color: var(--el-text-color-secondary); margin-top: 10px; text-align: center; }

.groups { display: flex; flex-direction: column; gap: 20px; }

.group-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 10px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}
.group-name { font-weight: 600; font-size: 15px; }

.icon-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
}

.icon-card { display: flex; flex-direction: column; }
.icon-thumb {
  height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--el-fill-color-light);
}
.icon-thumb img {
  max-width: 80%;
  max-height: 80%;
  object-fit: contain;
  image-rendering: pixelated;
}

.icon-info {
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.info-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.info-size { font-size: 13px; color: var(--el-text-color-secondary); }
</style>
