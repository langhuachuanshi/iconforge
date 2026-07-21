<script setup lang="ts">
import { ref } from 'vue'
import { ElMessage } from 'element-plus'
import { convertImagesToIco, blobToBase64, toDataUrl } from '../api/client'

interface ImageItem {
  /** base64（不含 data: 前缀），用于发给后端 */
  b64: string
  /** data url，用于 <img> 预览 */
  dataUrl: string
  name: string
}

const images = ref<ImageItem[]>([])
const processing = ref(false)
const sizes = ref<number[]>([16, 32, 48, 64, 128, 256])
const allSizes = [16, 32, 48, 64, 128, 256]

// el-upload before-upload：拦截 File，转 base64，不实际上传
async function onFilePicked(file: File) {
  try {
    const b64 = await blobToBase64(file)
    images.value.push({ b64, dataUrl: toDataUrl(b64), name: file.name })
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

async function handleConvert() {
  if (images.value.length === 0) {
    ElMessage.warning('请先添加图片')
    return
  }
  if (sizes.value.length === 0) {
    ElMessage.warning('请至少选择一个尺寸')
    return
  }
  processing.value = true
  try {
    await convertImagesToIco(
      images.value.map((i) => i.b64),
      sizes.value
    )
    ElMessage.success('已生成 ICO')
  } catch (e: any) {
    ElMessage.error('生成失败：' + (e?.message || e))
  } finally {
    processing.value = false
  }
}
</script>

<template>
  <div class="i2i-root">
    <h2 class="page-title">图片转 ICO</h2>

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
      <span class="hint">已添加 {{ images.length }} 张</span>
    </div>

    <!-- 图片列表 -->
    <div class="list-area" v-loading="processing">
      <el-empty v-if="images.length === 0" description="添加 PNG / JPG / BMP / WEBP 图片" />

      <div v-else class="thumb-list">
        <div v-for="(img, idx) in images" :key="idx" class="thumb-item">
          <img :src="img.dataUrl" :alt="img.name" />
          <div class="thumb-info">
            <span class="thumb-name" :title="img.name">{{ img.name }}</span>
            <el-button text size="small" type="danger" @click="removeImage(idx)">
              <el-icon><Delete /></el-icon>
            </el-button>
          </div>
        </div>
      </div>
    </div>

    <!-- 底部：尺寸选择 + 生成 -->
    <el-card class="options-card" shadow="never">
      <el-form label-position="top" size="small">
        <el-form-item label="ICO 尺寸">
          <el-checkbox-group v-model="sizes">
            <el-checkbox v-for="s in allSizes" :key="s" :value="s">{{ s }}×{{ s }}</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        <el-button
          type="primary"
          :loading="processing"
          :disabled="images.length === 0 || sizes.length === 0"
          @click="handleConvert"
          style="width: 100%"
        >
          <el-icon><Download /></el-icon>&nbsp;生成 ICO（每张图 × {{ sizes.length }} 尺寸）
        </el-button>
      </el-form>
    </el-card>
  </div>
</template>

<style scoped>
.i2i-root { display: flex; flex-direction: column; height: calc(100vh - 110px); }

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
  padding: 4px 8px;
}
.thumb-name {
  flex: 1;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.options-card { flex-shrink: 0; }
</style>
