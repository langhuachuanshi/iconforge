<script setup lang="ts">
import { ref, computed } from 'vue'
import { ElMessage } from 'element-plus'
import { extractIcons, saveIco, savePng, type ExtractedIcon } from '../api/client'

const icons = ref<ExtractedIcon[]>([])
const processing = ref(false)
const filePath = ref('')

/** 所有条目（每个尺寸一张） */
const allEntries = computed(() => icons.value)

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
  <div class="extract-root">
    <h2 class="page-title">图标提取</h2>

    <!-- 顶部操作栏 -->
    <div class="toolbar">
      <el-button type="primary" @click="handlePick" :loading="processing">
        <el-icon><FolderOpened /></el-icon>&nbsp;选择 PE 文件
      </el-button>
      <span v-if="filePath" class="file-path" :title="filePath">{{ filePath }}</span>
      <el-button v-if="filePath" text @click="load" :loading="processing">重新提取</el-button>
    </div>

    <!-- 提取结果 -->
    <div v-loading="processing" class="result-area">
      <el-empty v-if="!processing && icons.length === 0" description="选择 .exe / .dll / .ocx 文件以提取图标" />

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
  </div>
</template>

<style scoped>
.extract-root { display: flex; flex-direction: column; height: calc(100vh - 110px); }

.page-title { margin: 0 0 16px; font-size: 22px; }

.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}
.file-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  font-family: ui-monospace, monospace;
}

.result-area { flex: 1; overflow-y: auto; }

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
