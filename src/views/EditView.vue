<script setup lang="ts">
import { computed, ref, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  cropImage,
  exportIcon,
  removeBackground,
  downloadBgModel,
  listBgModels,
  getConfig,
  savePng,
  toDataUrl,
  blobToBase64,
  type BgModelEntry,
} from '../api/client'
import { useWorkspaceStore } from '../stores/workspace'

const workspace = useWorkspaceStore()
const image = ref('')
const processing = ref(false)
const downloading = ref(false)
const downloadPct = ref(0)
const isDirty = ref(false)

// ── 抠图模型 ──
const bgModels = ref<BgModelEntry[]>([])
const currentBgModelId = ref('')

const downloadedBgModels = computed(() => bgModels.value.filter(m => m.downloaded))

// ── 画布 ──
const canvasRef = ref<HTMLElement>()
const imgNatural = ref({ w: 0, h: 0 })
const scale = ref(1)
const panX = ref(0)
const panY = ref(0)
const isPanning = ref(false)
const panStart = ref({ x: 0, y: 0, px: 0, py: 0 })

// ── 撤回/重做 ──
const undoStack = ref<string[]>([])
const redoStack = ref<string[]>([])

function pushHistory() {
  if (!image.value) return
  undoStack.value.push(image.value)
  if (undoStack.value.length > 50) undoStack.value.shift()
  redoStack.value = []
  isDirty.value = true
}

function undo() {
  if (!undoStack.value.length) return
  redoStack.value.push(image.value)
  image.value = undoStack.value.pop()!
  workspace.setImage(image.value, '')
  isDirty.value = true
}

function redo() {
  if (!redoStack.value.length) return
  undoStack.value.push(image.value)
  image.value = redoStack.value.pop()!
  workspace.setImage(image.value, '')
  isDirty.value = true
}

// ── 缩放/拖拽 ──
function fitToCanvas() {
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect || !imgNatural.value.w) return
  const sx = (rect.width - 40) / imgNatural.value.w
  const sy = (rect.height - 40) / imgNatural.value.h
  scale.value = Math.min(sx, sy, 1)
  panX.value = (rect.width - imgNatural.value.w * scale.value) / 2
  panY.value = (rect.height - imgNatural.value.h * scale.value) / 2
}

function onCanvasWheel(e: WheelEvent) {
  e.preventDefault()
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return
  const mx = e.clientX - rect.left, my = e.clientY - rect.top
  const newScale = Math.max(0.1, Math.min(10, scale.value * (e.deltaY < 0 ? 1.1 : 0.9)))
  panX.value = mx - (mx - panX.value) * (newScale / scale.value)
  panY.value = my - (my - panY.value) * (newScale / scale.value)
  scale.value = newScale
}

function onCanvasMouseDown(e: MouseEvent) {
  if (e.button !== 0) return
  if (touchupActive.value) return // 触摸画布自己处理
  // 裁剪模式下直接拖拽平移图片（取景框固定不动）
  isPanning.value = true
  panStart.value = { x: e.clientX, y: e.clientY, px: panX.value, py: panY.value }
}

function onCanvasMouseMove(e: MouseEvent) {
  if (touchupActive.value) return // 触摸画布自己处理
  if (!isPanning.value) return
  panX.value = panStart.value.px + (e.clientX - panStart.value.x)
  panY.value = panStart.value.py + (e.clientY - panStart.value.y)
}

function onCanvasMouseUp() {
  isPanning.value = false
}

// ── 裁剪（取景框模式：框固定在画布中央，图片在背后缩放/平移） ──
const cropActive = ref(false)
const cropSize = ref(0.75) // 裁剪框占画布短边比例 0.3~1.0

// 取景框尺寸（CSS flexbox 自动居中）
const cropBoxStyle = computed(() => {
  const rect = canvasRef.value?.getBoundingClientRect()
  const side = Math.min(rect?.width || 400, rect?.height || 300) * cropSize.value
  return { width: `${side}px`, height: `${side}px` }
})

function startCrop() {
  if (!image.value) return
  cropActive.value = true
}

function cancelCrop() { cropActive.value = false }

async function confirmCrop() {
  if (!image.value) return
  const rect = canvasRef.value?.getBoundingClientRect()
  const cw = rect?.width || 400, ch = rect?.height || 300
  const side = Math.min(cw, ch) * cropSize.value
  // 取景框屏幕坐标（CSS flexbox 居中）→ 图像坐标
  const boxScreenX = (cw - side) / 2
  const boxScreenY = (ch - side) / 2
  const imgX = Math.round((boxScreenX - panX.value) / scale.value)
  const imgY = Math.round((boxScreenY - panY.value) / scale.value)
  const imgSide = Math.round(side / scale.value)
  // clamp 到图像边界
  const x = Math.max(0, imgX)
  const y = Math.max(0, imgY)
  const w = Math.min(imgSide, imgNatural.value.w - x)
  const h = Math.min(imgSide, imgNatural.value.h - y)

  pushHistory()
  processing.value = true; cropActive.value = false
  try {
    syncImage(await cropImage({ image: image.value, x, y, width: w, height: h }))
    ElMessage.success('裁剪完成')
  } catch (e: any) { ElMessage.error(`裁剪失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── 手动修补 ──
const touchupActive = ref(false)
const touchupPainting = ref(false)
const touchupMode = ref<'erase' | 'restore'>('erase')
const touchupBrushSize = ref(20)
const touchupCanvas = ref<HTMLCanvasElement>()
const previewCanvas = ref<HTMLCanvasElement>() // 抠图前的原图

function startTouchup() {
  if (!image.value) return
  touchupActive.value = true
  nextTick(() => {
    const tc = touchupCanvas.value; if (!tc) return
    tc.width = imgNatural.value.w; tc.height = imgNatural.value.h
    const ctx = tc.getContext('2d')!; ctx.clearRect(0, 0, tc.width, tc.height)
    const img = new Image()
    img.onload = () => { ctx.drawImage(img, 0, 0) }
    img.src = toDataUrl(image.value)
  })
}

function cancelTouchup() {
  touchupActive.value = false
}

async function applyTouchup() {
  if (!touchupCanvas.value || !image.value) return
  // 从修补画布导出新图（已含透明修改）
  pushHistory()
  processing.value = true
  const dataUrl = touchupCanvas.value.toDataURL('image/png')
  syncImage(dataUrl.split(',')[1])
  touchupActive.value = false
  processing.value = false
  ElMessage.success('修补已应用')
}

function startTouchupStroke(e: MouseEvent) {
  touchupPainting.value = true
  paintTouchupStroke(e)
}

function continueTouchupStroke(e: MouseEvent) {
  if (!touchupPainting.value) return
  paintTouchupStroke(e)
}

function paintTouchupStroke(e: MouseEvent) {
  const tc = touchupCanvas.value; if (!tc) return
  const rect = tc.getBoundingClientRect()
  // 在画布的内部坐标系中计算坐标
  const x = (e.clientX - rect.left) / scale.value
  const y = (e.clientY - rect.top) / scale.value
  const ctx = tc.getContext('2d')!; const r = touchupBrushSize.value

  if (touchupMode.value === 'erase') {
    ctx.globalCompositeOperation = 'destination-out'
  } else {
    ctx.globalCompositeOperation = 'source-over'
    ctx.fillStyle = '#fff' // 白底恢复
  }
  ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill()
  ctx.globalCompositeOperation = 'source-over'
}

function endTouchupStroke() { touchupPainting.value = false }


// ── 工具函数 ──
function syncImage(b64: string) {
  image.value = b64
  workspace.setImage(b64, '')
  const img = new Image()
  img.onload = () => {
    imgNatural.value = { w: img.naturalWidth, h: img.naturalHeight }
    nextTick(fitToCanvas)
  }
  img.src = toDataUrl(b64)
}

// ── 快捷键 ──
function onKeydown(e: KeyboardEvent) {
  if (!image.value && !undoStack.value.length) return
  if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
    e.preventDefault(); undo()
  } else if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
    e.preventDefault(); redo()
  }
}

onMounted(() => document.addEventListener('keydown', onKeydown))
onUnmounted(() => document.removeEventListener('keydown', onKeydown))

// ── 加载抠图模型列表 ──
async function loadBgModels() {
  try {
    bgModels.value = await listBgModels()
    const cur = bgModels.value.find(m => m.current)
    currentBgModelId.value = cur?.id ?? ''
  } catch { /* 静默 */ }
}

async function onBgModelChange(id: string) {
  // 切换当前模型到后端配置
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_config', { key: 'bg_model', value: id })
    currentBgModelId.value = id
    const m = bgModels.value.find(x => x.id === id)
    bgModels.value.forEach(x => x.current = x.id === id)
    ElMessage.success(`已切换为 ${m?.name ?? id}`)
  } catch (e: any) {
    ElMessage.error('切换失败：' + (e?.message || e))
  }
}

onMounted(loadBgModels)

// ── 初始化 ──
watch(() => workspace.currentImage, (val) => {
  if (val) {
    image.value = val
    const img = new Image()
    img.onload = () => {
      imgNatural.value = { w: img.naturalWidth, h: img.naturalHeight }
      nextTick(fitToCanvas)
    }
    img.src = toDataUrl(val)
  }
}, { immediate: true })

// ── 文件操作 ──
async function openFile(file: File) {
  if (isDirty.value) {
    try { await ElMessageBox.confirm('当前图片有未保存的修改，是否丢弃？', '确认', { type: 'warning' }) }
    catch { return false }
  }
  syncImage(await blobToBase64(file))
  isDirty.value = false
  undoStack.value = []; redoStack.value = []
  return false // 阻止 el-upload 默认上传
}

async function handleSave() {
  if (!image.value) return
  try {
    const saved = await savePng(image.value, 'icon.png')
    if (!saved) return // 用户取消
    isDirty.value = false
    ElMessage.success('已保存')
  } catch (e: any) {
    ElMessage.error('保存失败：' + (e?.message || e))
  }
}

async function handleClose() {
  if (!image.value) return
  if (isDirty.value) {
    try { await ElMessageBox.confirm('当前图片有未保存的修改，是否关闭？', '确认关闭', { type: 'warning' }) }
    catch { return }
  }
  image.value = ''
  workspace.clear()
  isDirty.value = false
  undoStack.value = []; redoStack.value = []
}

// ── 智能抠图 ──
async function handleRemoveBg() {
  if (!image.value) return
  // 当前模型没下载：提示是否下载
  const cur = bgModels.value.find(m => m.id === currentBgModelId.value)
  const downloaded = cur?.downloaded ?? false
  if (!downloaded) {
    try {
      await ElMessageBox.confirm(
        cur ? `当前模型「${cur.name}」未下载，是否下载？（${cur.size}）` : '当前模型未下载，是否下载？',
        '下载模型',
        { confirmButtonText: '下载', cancelButtonText: '取消', type: 'info' }
      )
    } catch { return }
    downloading.value = true; downloadPct.value = 0
    try {
      await downloadBgModel(pct => { downloadPct.value = Math.round(pct) })
      await loadBgModels()
      ElMessage.success('下载完成')
    } catch (e: any) { ElMessage.error(`下载失败：${e?.message || e}`); return }
    finally { downloading.value = false }
  }

  pushHistory()
  processing.value = true
  try {
    syncImage(await removeBackground(image.value))
    ElMessage.success('智能抠图完成')
  } catch (e: any) { ElMessage.error(`抠图失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── 导出 ──
const pngSizes = ref([16, 32, 48, 64, 128, 256, 512])
const icoSizes = ref([16, 32, 48, 64, 128, 256])
const pngAll = [16, 32, 48, 64, 128, 256, 512]
const icoAll = [16, 32, 48, 64, 128, 256]

async function handleExport() {
  if (!image.value) return
  processing.value = true
  try { await exportIcon(image.value, pngSizes.value, icoSizes.value); ElMessage.success('导出完成') }
  catch (e: any) { if (e) ElMessage.error(`导出失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── computed ──
const imageTransform = computed(() => `translate(${panX.value}px, ${panY.value}px) scale(${scale.value})`)
</script>

<template>
  <div class="edit-root">
    <!-- 顶部栏 -->
    <div class="top-bar">
      <div class="top-left">
        <el-upload :show-file-list="false" :before-upload="openFile" accept="image/*">
          <el-button size="small"><el-icon><FolderOpened /></el-icon> 打开</el-button>
        </el-upload>
      </div>

      <div class="top-center undo-redo">
        <el-button size="small" :disabled="!undoStack.length" @click="undo" title="撤回 Ctrl+Z">
          <el-icon><RefreshLeft /></el-icon>
        </el-button>
        <el-button size="small" :disabled="!redoStack.length" @click="redo" title="重做 Ctrl+Y">
          <el-icon><RefreshRight /></el-icon>
        </el-button>
      </div>

      <div class="top-right">
        <el-button size="small" text @click="fitToCanvas">适应窗口</el-button>
        <span class="zoom-label">{{ Math.round(scale * 100) }}%</span>
        <el-button size="small" @click="handleSave" :disabled="!image"><el-icon><Download /></el-icon> 保存</el-button>
        <el-button size="small" @click="handleClose" :disabled="!image"><el-icon><Close /></el-icon> 关闭</el-button>
      </div>
    </div>

    <!-- 空状态 -->
    <el-empty v-if="!image" description="打开或生成一张图片开始编辑" class="empty-state">
      <el-upload :show-file-list="false" :before-upload="openFile" accept="image/*">
        <el-button type="primary">打开图片</el-button>
      </el-upload>
    </el-empty>

    <!-- 编辑区 -->
    <div v-else class="editor-body">
      <!-- 画布 -->
      <div
        ref="canvasRef"
        class="canvas"
        v-loading="processing"
        @wheel="onCanvasWheel"
        @mousedown="onCanvasMouseDown"
        @mousemove="onCanvasMouseMove"
        @mouseup="onCanvasMouseUp"
        @mouseleave="onCanvasMouseUp"
      >
        <div class="canvas-bg checkerboard" />
        <img :src="toDataUrl(image)" class="canvas-img" :style="{ transform: imageTransform }" draggable="false" />

        <!-- 裁剪取景框（固定在画布中央，图片在背后缩放平移） -->
        <div v-if="cropActive" class="crop-overlay">
          <div class="crop-viewfinder" :style="cropBoxStyle">
            <div class="crop-grid-h" v-for="i in 2" :key="'h'+i" :style="{ top: `${(100/3)*i}%` }" />
            <div class="crop-grid-v" v-for="i in 2" :key="'v'+i" :style="{ left: `${(100/3)*i}%` }" />
          </div>
        </div>

        <!-- 修补画布 -->
        <canvas
          v-if="touchupActive"
          ref="touchupCanvas"
          class="touchup-canvas"
          :style="{ transform: imageTransform, transformOrigin: '0 0' }"
          @mousedown="startTouchupStroke"
          @mousemove="continueTouchupStroke"
          @mouseup="endTouchupStroke"
          @mouseleave="endTouchupStroke"
          @wheel.prevent="onCanvasWheel"
        />

      </div>

      <!-- 右侧工具栏 -->
      <div class="side-panel">
        <!-- 编辑 -->
        <el-card>
          <template #header>编辑</template>
          <template v-if="!cropActive && !touchupActive">
            <el-button :disabled="processing" @click="startCrop" style="width:100%">
              <el-icon><Crop /></el-icon> 裁剪
            </el-button>
            <p class="tool-desc">自由裁剪，九宫格辅助构图</p>
          </template>
          <template v-else-if="cropActive">
            <div style="margin-bottom:8px">
              <span class="tool-desc">取景框：{{ Math.round(cropSize * 100) }}%</span>
              <el-slider v-model="cropSize" :min="0.3" :max="1.0" :step="0.05" size="small" />
            </div>
            <div class="btn-row"><el-button type="primary" @click="confirmCrop" style="flex:1">确认</el-button>
            <el-button @click="cancelCrop" style="flex:1">取消</el-button></div>
            <p class="tool-desc">滚轮缩放，拖拽移动图片，框内为裁剪结果</p>
          </template>
          <template v-else>
            <div style="margin-bottom:8px">
              <el-radio-group v-model="touchupMode" size="small">
                <el-radio-button value="erase">擦除</el-radio-button>
                <el-radio-button value="restore">恢复</el-radio-button>
              </el-radio-group>
            </div>
            <div style="margin-bottom:8px">
              <span class="tool-desc">画笔大小：{{ touchupBrushSize }}px</span>
              <el-slider v-model="touchupBrushSize" :min="2" :max="80" size="small" />
            </div>
            <div class="btn-row"><el-button type="primary" @click="applyTouchup" :loading="processing" style="flex:1">应用</el-button>
            <el-button @click="cancelTouchup" style="flex:1">取消</el-button></div>
          </template>
          <el-divider v-if="!cropActive && !touchupActive" />
          <template v-if="!cropActive && !touchupActive">
            <div class="bg-model-picker">
              <span class="tool-desc">抠图模型</span>
              <el-select
                :model-value="currentBgModelId"
                size="small"
                style="width:100%; margin-top:4px"
                placeholder="无可用模型"
                @change="onBgModelChange"
              >
                <el-option
                  v-for="m in downloadedBgModels"
                  :key="m.id"
                  :value="m.id"
                  :label="m.name"
                />
              </el-select>
              <p v-if="!downloadedBgModels.length" class="tool-desc" style="color: var(--el-color-warning)">
                尚未下载任何模型，点击下方按钮下载
              </p>
            </div>
            <el-button :disabled="processing || downloading || !downloadedBgModels.length" @click="handleRemoveBg" style="width:100%; margin-top:8px">
              <el-icon><MagicStick /></el-icon> 智能抠图
            </el-button>
            <el-progress v-if="downloading" :percentage="downloadPct" :stroke-width="6" style="margin-top:8px" />
            <p class="tool-desc">AI 本地抠图（首次自动下载模型）</p>
            <el-divider />
            <el-button :disabled="processing || !image" @click="startTouchup" style="width:100%">
              <el-icon><Brush /></el-icon> 手动修补
            </el-button>
            <p class="tool-desc">画笔擦除/恢复透明区域</p>
          </template>
        </el-card>

        <!-- 导出 -->
        <el-card style="margin-top:12px">
          <template #header>导出</template>
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
            <el-button type="primary" :loading="processing" :disabled="!pngSizes.length && !icoSizes.length" @click="handleExport" style="width:100%">
              <el-icon><Download /></el-icon> 导出 ZIP
            </el-button>
          </el-form>
        </el-card>
      </div>
    </div>
  </div>
</template>

<style scoped>
.edit-root { display: flex; flex-direction: column; height: calc(100vh - 110px); }

/* 顶部栏 */
.top-bar {
  display: flex; align-items: center; margin-bottom: 8px; flex-shrink: 0; gap: 8px;
}
.top-left { display: flex; gap: 4px; }
.top-center { flex: 1; display: flex; justify-content: center; }
.undo-redo { gap: 0; }
.undo-redo .el-button { border-radius: 4px; margin-left: 0; }
.undo-redo .el-button + .el-button { border-left: 1px solid var(--el-border-color); border-top-left-radius: 0; border-bottom-left-radius: 0; }
.undo-redo .el-button:first-child { border-top-right-radius: 0; border-bottom-right-radius: 0; }
.top-right { display: flex; gap: 4px; align-items: center; }

.empty-state { flex: 1; display: flex; align-items: center; justify-content: center; }

.editor-body { flex: 1; display: flex; gap: 12px; min-height: 0; }

/* 画布 */
.canvas {
  flex: 1; position: relative; overflow: hidden; border-radius: 6px;
  cursor: grab; min-width: 0;
}
.canvas:active { cursor: grabbing; }
.canvas-bg { position: absolute; inset: 0; }
.checkerboard {
  background-image:
    linear-gradient(45deg, var(--el-border-color-lighter) 25%, transparent 25%),
    linear-gradient(-45deg, var(--el-border-color-lighter) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, var(--el-border-color-lighter) 75%),
    linear-gradient(-45deg, transparent 75%, var(--el-border-color-lighter) 75%);
  background-size: 20px 20px; background-position: 0 0, 0 10px, 10px -10px, -10px 0px;
  background-color: var(--el-fill-color-lighter);
}

.canvas-img { position: absolute; top: 0; left: 0; transform-origin: 0 0; }

/* 裁剪取景框（flexbox 居中，不拦截鼠标事件） */
.crop-overlay {
  position: absolute; inset: 0; z-index: 5; pointer-events: none;
  display: flex; align-items: center; justify-content: center;
}
.crop-viewfinder {
  position: relative;
  outline: 2px solid var(--el-color-primary); outline-offset: -1px;
  box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.55);
}
.crop-grid-h { position: absolute; left: 0; right: 0; border-top: 1px dashed rgba(255,255,255,0.6); }
.crop-grid-v { position: absolute; top: 0; bottom: 0; border-left: 1px dashed rgba(255,255,255,0.6); }

/* 修补画布（覆盖在图片上，接收画笔操作） */
.touchup-canvas { position: absolute; top: 0; left: 0; pointer-events: auto; cursor: none; }

.zoom-label { font-size: 12px; color: var(--el-text-color-secondary); min-width: 36px; text-align: center; }

/* 右侧面板 */
.side-panel { width: 220px; flex-shrink: 0; overflow-y: auto; }
.tool-desc { font-size: 12px; color: var(--el-text-color-secondary); margin: 6px 0 0; }
.btn-row { display: flex; gap: 6px; margin-top: 4px; }
</style>
