<script setup lang="ts">
import { onActivated, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { exportIcon, blobToBase64, toDataUrl } from '../api/client'
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
const pngSizes = ref([16, 32, 48, 64, 128, 256, 512])
const icoSizes = ref([16, 32, 48, 64, 128, 256])
const pngAll = [16, 32, 48, 64, 128, 256, 512]
const icoAll = [16, 32, 48, 64, 128, 256]

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

function removeImage(idx: number) {
  images.value.splice(idx, 1)
}

function clearAll() {
  images.value = []
}

async function handleExport() {
  if (images.value.length === 0) {
    ElMessage.warning('请先添加图片')
    return
  }
  if (pngSizes.value.length === 0 && icoSizes.value.length === 0) {
    ElMessage.warning('请至少选择一个尺寸')
    return
  }
  processing.value = true
  let ok = 0
  let fail = 0
  try {
    for (const img of images.value) {
      try {
        await exportIcon(img.b64, pngSizes.value, icoSizes.value)
        ok++
      } catch (e: any) {
        // 用户在保存对话框点取消时，e 可能为空，不算失败
        if (e) { fail++; console.error(`导出 ${img.name} 失败:`, e?.message || e) }
      }
    }
    if (ok > 0) ElMessage.success(`已导出 ${ok} 个文件${fail > 0 ? `（${fail} 个失败）` : ''}`)
    else if (fail > 0) ElMessage.error(`导出失败 ${fail} 个`)
  } finally {
    processing.value = false
  }
}
</script>

<template>
  <div class="export-root">
    <h2 class="page-title">导出图标</h2>

    <!-- 顶部：添加图片 -->
    <div class="toolbar">
      <el-upload
        :show-file-list="false"
        :before-upload="onFilePicked"
        accept="image/png,image/jpeg,image/bmp,image/webp"
        multiple
      >
        <el-button type="primary">
          <el-icon><Plus /></el-icon>&nbsp;添加图片
        </el-button>
      </el-upload>
      <el-button v-if="images.length" text @click="clearAll">清空</el-button>
      <span class="hint" v-if="images.length === 0 && !workspace.currentImage">
        先在「编辑」页准备好图片，或点上方按钮添加本地图片
      </span>
      <span class="hint" v-else>共 {{ images.length }} 张</span>
    </div>

    <!-- 图片列表 -->
    <div class="list-area" v-loading="processing">
      <el-empty
        v-if="images.length === 0"
        description="还没有可导出的图片"
      />

      <div v-else class="thumb-list">
        <div v-for="(img, idx) in images" :key="idx" class="thumb-item">
          <img :src="img.dataUrl" :alt="img.name" />
          <div class="thumb-info">
            <span class="thumb-name" :title="img.name">
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

    <!-- 底部：尺寸选择 + 导出 -->
    <el-card class="options-card" shadow="never">
      <el-form label-position="top" size="small">
        <el-form-item label="PNG 尺寸">
          <el-checkbox-group v-model="pngSizes">
            <el-checkbox v-for="s in pngAll" :key="s" :value="s">{{ s }}</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-form-item label="ICO 尺寸">
          <el-checkbox-group v-model="icoSizes">
            <el-checkbox v-for="s in icoAll" :key="s" :value="s">{{ s }}</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-button
          type="primary"
          :loading="processing"
          :disabled="images.length === 0 || (pngSizes.length === 0 && icoSizes.length === 0)"
          @click="handleExport"
          style="width: 100%"
        >
          <el-icon><Download /></el-icon>&nbsp;导出（每张图 × {{ pngSizes.length + icoSizes.length }} 尺寸，生成 ZIP）
        </el-button>
      </el-form>
    </el-card>
  </div>
</template>

<style scoped>
.export-root { display: flex; flex-direction: column; height: calc(100vh - 110px); }

.page-title { margin: 0 0 16px; font-size: 22px; }

.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}
.hint { color: var(--el-text-color-secondary); font-size: 13px; }

.list-area { flex: 1; overflow-y: auto; min-height: 200px; }

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
  height: 120px;
  object-fit: contain;
  background: var(--el-fill-color-light);
}
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

.options-card { flex-shrink: 0; }
</style>
