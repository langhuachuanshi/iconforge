<script setup lang="ts">
import { computed, ref, watch, nextTick, onMounted, onActivated, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  cropImage,
  removeBackground,
  removeBackgroundCloud,
  removeColor,
  edgeRefine,
  smartCrop,
  applyShapeMask,
  adjustColor,
  downloadBgModel,
  listBgModels,
  getConfig,
  setConfig,
  savePng,
  saveIconVersion,
  toDataUrl,
  blobToBase64,
  type BgModelEntry,
} from '../api/client'
import { useWorkspaceStore } from '../stores/workspace'

const router = useRouter()
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

// ── 抠图引擎（local 本地模型 / cloud 云端 remove.bg）──
const engine = ref<'local' | 'cloud'>('local')
const cloudKeyConfigured = ref(false)
const currentCloudModel = ref<'common' | 'commodity'>('common')

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
  // 画布 DOM 尚未布局好（刚从生成页跳来、或 v-else 刚渲染）时 rect 会偏小甚至为 0，
  // 此时算出的 scale 为负或 0（表现为 -4%）。防御：rect 太小则跳过，等下次调用。
  if (!rect || !imgNatural.value.w || rect.width < 80 || rect.height < 80) return
  const sx = (rect.width - 40) / imgNatural.value.w
  const sy = (rect.height - 40) / imgNatural.value.h
  const s = Math.min(sx, sy, 1)
  if (!(s > 0)) return // scale 非正数则不赋值
  scale.value = s
  panX.value = (rect.width - imgNatural.value.w * s) / 2
  panY.value = (rect.height - imgNatural.value.h * s) / 2
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
  // 去底色模式：点击画布拾取像素颜色（吸管）
  if (colorActive.value) {
    pickColorAt(e.clientX, e.clientY)
    return
  }
  // 裁剪模式下直接拖拽平移图片（取景框固定不动）
  isPanning.value = true
  panStart.value = { x: e.clientX, y: e.clientY, px: panX.value, py: panY.value }
}

/** 吸管：屏幕坐标 → 图像像素 → 取该像素颜色，写入 bgColor */
function pickColorAt(screenX: number, screenY: number) {
  if (!image.value || !canvasRef.value) return
  const rect = canvasRef.value.getBoundingClientRect()
  // 屏幕坐标 → 图像坐标（与裁剪 confirmCrop 同一套换算）
  const imgX = Math.round((screenX - rect.left - panX.value) / scale.value)
  const imgY = Math.round((screenY - rect.top - panY.value) / scale.value)
  if (imgX < 0 || imgY < 0 || imgX >= imgNatural.value.w || imgY >= imgNatural.value.h) return
  // 用隐藏 canvas 取像素
  const c = document.createElement('canvas')
  c.width = imgNatural.value.w
  c.height = imgNatural.value.h
  const ctx = c.getContext('2d')!
  const img = new Image()
  img.onload = () => {
    ctx.drawImage(img, 0, 0)
    const p = ctx.getImageData(imgX, imgY, 1, 1).data
    const hex = '#' + [p[0], p[1], p[2]].map((v) => v.toString(16).padStart(2, '0')).join('')
    bgColor.value = hex
  }
  img.src = toDataUrl(image.value)
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

// ── 工具状态机（统一管理所有工具的激活状态，PS 风格）──
type ToolId = 'crop' | 'removeColor' | 'removeBg' | 'touchup' | 'smartCrop' | 'edgeRefine' | 'shapeMask' | 'adjustColor'
const activeTool = ref<ToolId | null>(null)

// 左侧工具栏列表（顺序 = 显示顺序）
// 工具按工作流分组：抠图 → 裁剪 → 调整装饰
const toolGroups: { label: string; items: { id: ToolId; name: string; icon: string }[] }[] = [
  {
    label: '抠图',
    items: [
      { id: 'removeBg', name: '智能抠图', icon: 'MagicStick' },
      { id: 'removeColor', name: '去底色', icon: 'Aim' },
      { id: 'touchup', name: '手动修补', icon: 'Brush' },
    ],
  },
  {
    label: '裁剪',
    items: [
      { id: 'crop', name: '自由裁剪', icon: 'Crop' },
      { id: 'smartCrop', name: '智能裁剪', icon: 'ScaleToOriginal' },
    ],
  },
  {
    label: '调整',
    items: [
      { id: 'edgeRefine', name: '边缘净化', icon: 'Filter' },
      { id: 'shapeMask', name: '形状遮罩', icon: 'PieChart' },
      { id: 'adjustColor', name: '调色', icon: 'Sunny' },
    ],
  },
]

function toolName(id: ToolId): string {
  for (const g of toolGroups) {
    const t = g.items.find((t) => t.id === id)
    if (t) return t.name
  }
  return ''
}

// 桥接现有布尔：画布 v-if / onKeydown / touchup 等逻辑依赖这些，无需改
const cropActive = computed(() => activeTool.value === 'crop')
const colorActive = computed(() => activeTool.value === 'removeColor')
const touchupActive = computed(() => activeTool.value === 'touchup')

// Drawer 显隐绑定到 activeTool（touchup 不走抽屉，控件直接铺在画布上）
const drawerVisible = computed({
  get: () => activeTool.value !== null && activeTool.value !== 'touchup',
  set: (v: boolean) => { if (!v) closeTool() },
})

function selectTool(tool: ToolId) {
  if (!image.value) return
  // 切换前若已有交互工具激活，先清理其画布状态
  if (activeTool.value && activeTool.value !== tool) {
    cleanupActiveTool()
  }
  const willActivate = activeTool.value !== tool
  activeTool.value = willActivate ? tool : null
  // 进入修补工具时初始化修补画布（设尺寸 + 画底图）
  if (willActivate && tool === 'touchup') {
    initTouchupCanvas()
  }
}

function closeTool() {
  cleanupActiveTool()
  activeTool.value = null
}

/** 清理当前交互工具的画布状态。
 *  交互工具的画布元素靠 v-if 绑定 activeTool，切换时自动卸载；这里只清理残留状态。 */
function cleanupActiveTool() {
  // 手动修补：取消未完成的预览 rAF + 重置光标 + 释放离屏原图引用
  if (previewRaf) { cancelAnimationFrame(previewRaf); previewRaf = 0 }
  previewUrl.value = ''
  brushCursor.value.visible = false
  originalCanvas = null
}

// ── 裁剪（取景框模式：框固定在画布中央，图片在背后缩放/平移） ──
const cropSize = ref(0.75) // 裁剪框占画布短边比例 0.3~1.0

// 取景框尺寸（CSS flexbox 自动居中）
const cropBoxStyle = computed(() => {
  const rect = canvasRef.value?.getBoundingClientRect()
  const side = Math.min(rect?.width || 400, rect?.height || 300) * cropSize.value
  return { width: `${side}px`, height: `${side}px` }
})



function cancelCrop() { activeTool.value = null }

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
  processing.value = true; activeTool.value = null
  try {
    syncImage(await cropImage({ image: image.value, x, y, width: w, height: h }))
    ElMessage.success('裁剪完成')
  } catch (e: any) { ElMessage.error(`裁剪失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── 去底色（魔棒/色键）──（colorActive 见上方工具状态机 computed）
const bgColor = ref('#ffffff')
const colorTolerance = ref(60)


function cancelRemoveColor() {
  activeTool.value = null
}

// hex (#rrggbb) → [r,g,b]
function hexToRgb(hex: string): [number, number, number] {
  const m = hex.replace('#', '')
  return [
    parseInt(m.slice(0, 2), 16) || 0,
    parseInt(m.slice(2, 4), 16) || 0,
    parseInt(m.slice(4, 6), 16) || 0,
  ]
}

async function applyRemoveColor() {
  if (!image.value) return
  pushHistory()
  processing.value = true
  try {
    const result = await removeColor(image.value, hexToRgb(bgColor.value), colorTolerance.value)
    syncImage(result)
    ElMessage.success('去底色完成')
  } catch (e: any) {
    ElMessage.error(`去底色失败：${e?.message || e}`)
  } finally {
    processing.value = false
    activeTool.value = null
  }
}

// ── 手动修补（美图秀秀风格标记式抠图：去除/保留笔刷 + 反选 + 重置 + 实时预览） ──
// （touchupActive 见上方工具状态机 computed）
const touchupPainting = ref(false)
const touchupMode = ref<'remove' | 'keep'>('remove')
const touchupBrushSize = ref(20)
const touchupCanvas = ref<HTMLCanvasElement>()
// 红色遮罩画布：叠加在修补画布上，半透明红覆盖「保留（可见）」区域，让用户看清涂抹范围
const maskCanvas = ref<HTMLCanvasElement>()
// 离屏原图 canvas：保留/反选/重置的像素来源（始终是 pristine 原图，从不修改）
let originalCanvas: HTMLCanvasElement | null = null

// 右侧实时预览 dataURL（rAF 节流刷新，不进 workspace store，避免污染撤销栈）
const previewUrl = ref('')
let previewRaf = 0
// 笔刷光标跟随圆（去除红 / 保留绿）
const brushCursor = ref({ x: 0, y: 0, visible: false })

/** 初始化修补画布 + 离屏原图 + 红色遮罩画布，进入修补工具时调 */
function initTouchupCanvas() {
  nextTick(() => {
    const tc = touchupCanvas.value; if (!tc) return
    tc.width = imgNatural.value.w; tc.height = imgNatural.value.h
    const ctx = tc.getContext('2d')!; ctx.clearRect(0, 0, tc.width, tc.height)
    // 红色遮罩画布同尺寸
    const mc = maskCanvas.value; if (mc) {
      mc.width = imgNatural.value.w; mc.height = imgNatural.value.h
    }
    const img = new Image()
    img.onload = () => {
      // 主修补画布：画底图（可见区 = 原图）
      ctx.drawImage(img, 0, 0)
      // 离屏原图：建一份 pristine 原图，保留/反选/重置从它取像素
      originalCanvas = document.createElement('canvas')
      originalCanvas.width = imgNatural.value.w
      originalCanvas.height = imgNatural.value.h
      originalCanvas.getContext('2d')!.drawImage(img, 0, 0)
      schedulePreview()
      // 绘画区图片自动适中显示（布局变了，需等 DOM 稳定后重算缩放）
      nextTick(() => requestAnimationFrame(fitToCanvas))
    }
    img.src = toDataUrl(image.value)
  })
}

/** 重置修补画布：清空 + 重新画原图（全部恢复可见） */
function resetTouchup() {
  const tc = touchupCanvas.value; if (!tc || !originalCanvas) return
  const ctx = tc.getContext('2d')!
  ctx.globalCompositeOperation = 'source-over'
  ctx.clearRect(0, 0, tc.width, tc.height)
  ctx.drawImage(originalCanvas, 0, 0)
  schedulePreview()
}

/** 反选：可见区 ↔ 透明区 精确对调
 *  算法：tmpCanvas 画原图 → destination-out 抠掉「当前可见区」→ 剩下的就是「原透明区」的像素，
 *  再贴回主画布即完成对调。 */
function invertTouchup() {
  const tc = touchupCanvas.value; if (!tc || !originalCanvas) return
  const w = tc.width, h = tc.height
  const tmp = document.createElement('canvas')
  tmp.width = w; tmp.height = h
  const tctx = tmp.getContext('2d')!
  // ① 画原图
  tctx.drawImage(originalCanvas, 0, 0)
  // ② 抠掉当前主画布可见区 → tmp 剩下「原图有、当前没有」的像素 = 原透明区
  tctx.globalCompositeOperation = 'destination-out'
  tctx.drawImage(tc, 0, 0)
  // ③ 贴回主画布
  const ctx = tc.getContext('2d')!
  ctx.globalCompositeOperation = 'source-over'
  ctx.clearRect(0, 0, w, h)
  ctx.drawImage(tmp, 0, 0)
  schedulePreview()
}

async function applyTouchup() {
  if (!touchupCanvas.value || !image.value) return
  // 从修补画布导出新图（已含透明修改）
  pushHistory()
  processing.value = true
  const dataUrl = touchupCanvas.value.toDataURL('image/png')
  syncImage(dataUrl.split(',')[1])
  activeTool.value = null
  processing.value = false
  ElMessage.success('修补已应用')
}

function startTouchupStroke(e: MouseEvent) {
  touchupPainting.value = true
  updateBrushCursor(e)
  paintTouchupStroke(e)
}

function continueTouchupStroke(e: MouseEvent) {
  // 无论是否在画，都更新光标位置（鼠标在触摸 canvas 上移动即跟随）
  updateBrushCursor(e)
  if (!touchupPainting.value) return
  paintTouchupStroke(e)
}

/** 单笔触：去除 → destination-out 画圆挖透明；保留 → clip 圆 + 从原图取像素（修复原白色 bug） */
function paintTouchupStroke(e: MouseEvent) {
  const tc = touchupCanvas.value; if (!tc) return
  // 触摸 canvas 带 transform(translate+scale)，用它的 getBoundingClientRect 会把变换算两次导致偏移。
  // 改用容器（.tt-paint，无 transform）的 rect，再手动减 panX/panY、除以 scale，得到画布内部坐标。
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return
  const x = (e.clientX - rect.left - panX.value) / scale.value
  const y = (e.clientY - rect.top - panY.value) / scale.value
  const ctx = tc.getContext('2d')!; const r = touchupBrushSize.value

  if (touchupMode.value === 'remove') {
    ctx.globalCompositeOperation = 'destination-out'
    ctx.fillStyle = '#000'
    ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill()
  } else if (originalCanvas) {
    // 保留：从离屏原图取像素，恢复真实 RGB（修掉原 #fff 白色 bug）
    ctx.globalCompositeOperation = 'source-over'
    ctx.save()
    ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.clip()
    ctx.drawImage(originalCanvas, 0, 0)
    ctx.restore()
  }
  ctx.globalCompositeOperation = 'source-over'
  schedulePreview()
}

/** 鼠标屏幕坐标 → 更新笔刷光标圆位置（相对容器，光标是不带 transform 的容器子元素），并置可见 */
function updateBrushCursor(e: MouseEvent) {
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return
  brushCursor.value.x = e.clientX - rect.left
  brushCursor.value.y = e.clientY - rect.top
  brushCursor.value.visible = true
}

function endTouchupStroke() { touchupPainting.value = false }

/** rAF 节流：每帧最多一次 —— 刷新预览 dataURL + 重绘红色保留遮罩 */
function schedulePreview() {
  if (previewRaf) return
  previewRaf = requestAnimationFrame(() => {
    previewRaf = 0
    const tc = touchupCanvas.value
    if (tc) previewUrl.value = tc.toDataURL('image/png')
    // 红色遮罩：清空 → source-in 方式把修补画布的 alpha 通道「染成半透明红」。
    // 保留区（可见=alpha>0）显红，去除区（透明=alpha=0）无遮罩露出棋盘格。
    const mc = maskCanvas.value
    if (mc && tc && mc.width === tc.width) {
      const mctx = mc.getContext('2d')!
      mctx.globalCompositeOperation = 'source-over'
      mctx.clearRect(0, 0, mc.width, mc.height)
      mctx.drawImage(tc, 0, 0) // 拿到修补画布的形状（带 alpha）
      mctx.globalCompositeOperation = 'source-in'
      mctx.fillStyle = 'rgba(220, 40, 40, 0.45)'
      mctx.fillRect(0, 0, mc.width, mc.height)
      mctx.globalCompositeOperation = 'source-over'
    }
  })
}


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
  // 裁剪模式下方向键微调图片位置（取景框固定，移图片改变框内内容）
  if (cropActive.value && ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
    e.preventDefault()
    const step = e.shiftKey ? 30 : 5
    if (e.key === 'ArrowLeft') panX.value += step
    else if (e.key === 'ArrowRight') panX.value -= step
    else if (e.key === 'ArrowUp') panY.value += step
    else if (e.key === 'ArrowDown') panY.value -= step
    return
  }
  if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
    e.preventDefault(); undo()
  } else if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) {
    e.preventDefault(); redo()
  }
}

onMounted(() => document.addEventListener('keydown', onKeydown))
onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown)
  if (previewRaf) cancelAnimationFrame(previewRaf)
})

// ── 加载抠图模型列表 ──
async function loadBgModels() {
  try {
    bgModels.value = await listBgModels()
    const cur = bgModels.value.find(m => m.current)
    currentBgModelId.value = cur?.id ?? ''
  } catch { /* 静默 */ }
}

// ── 加载抠图引擎配置 ──
async function loadEngineConfig() {
  try {
    const cfg = await getConfig()
    engine.value = cfg.bg_engine === 'cloud' ? 'cloud' : 'local'
    cloudKeyConfigured.value = !!(cfg.aliyun_ak && cfg.aliyun_sk)
    currentCloudModel.value = cfg.cloud_model === 'commodity' ? 'commodity' : 'common'
  } catch { /* 静默，默认 local */ }
}

async function onBgModelChange(id: string) {
  // 切换当前模型到后端配置
  try {
    await setConfig('bg_model', id)
    currentBgModelId.value = id
    const m = bgModels.value.find(x => x.id === id)
    bgModels.value.forEach(x => x.current = x.id === id)
    ElMessage.success(`已切换为 ${m?.name ?? id}`)
  } catch (e: any) {
    ElMessage.error('切换失败：' + (e?.message || e))
  }
}

async function onCloudModelChange(val: 'common' | 'commodity') {
  try {
    await setConfig('cloud_model', val)
    currentCloudModel.value = val
    ElMessage.success(`已切换为 ${val === 'commodity' ? '商品分割' : '通用分割'}`)
  } catch (e: any) {
    ElMessage.error('切换失败：' + (e?.message || e))
  }
}

async function onEngineChange(val: 'local' | 'cloud') {
  try {
    await setConfig('bg_engine', val)
    engine.value = val
  } catch (e: any) {
    ElMessage.error('切换引擎失败：' + (e?.message || e))
    engine.value = val === 'cloud' ? 'local' : 'cloud' // 回滚 UI
  }
}

function goToSettings() {
  router.push('/settings')
}

onMounted(loadBgModels)
onMounted(loadEngineConfig)

// keep-alive 下每次进入编辑页都重新自适应窗口（图可能换了、或窗口尺寸变了）
onActivated(() => {
  if (image.value && imgNatural.value.w) {
    nextTick(() => requestAnimationFrame(fitToCanvas))
  }
})

// ── 初始化 ──
watch(() => workspace.currentImage, (val) => {
  if (val) {
    image.value = val
    const img = new Image()
    img.onload = () => {
      imgNatural.value = { w: img.naturalWidth, h: img.naturalHeight }
      // 双层 nextTick + rAF：确保 v-else 编辑区 DOM 完成布局后再算缩放，
      // 避免刚跳转时 rect 偏小导致 scale 异常（负数/0）。
      nextTick(() => requestAnimationFrame(fitToCanvas))
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

// ── 拖拽打开图片 ──
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
  // relatedTarget 为 null 表示真正离开窗口/容器，避免子元素间移动误触发
  if (e.relatedTarget === null) dragOver.value = false
}
async function onDrop(e: DragEvent) {
  e.preventDefault()
  dragOver.value = false
  const files = Array.from(e.dataTransfer?.files ?? [])
  const img = files.find((f) => f.type.startsWith('image/'))
  if (img) await openFile(img)
}

async function handleSave() {
  if (!image.value) return
  try {
    // 有 iconId（图来自历史/生成）→ 存为该图标的编辑版本（工程存档）
    if (workspace.currentIconId) {
      const meta = await saveIconVersion(workspace.currentIconId, image.value)
      isDirty.value = false
      ElMessage.success(`已存为版本 ${meta.versionNo}`)
      return
    }
    // 无 iconId（本地打开的图）→ 弹对话框存单 PNG（原行为）
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

  // 云端引擎分支
  if (engine.value === 'cloud') {
    if (!cloudKeyConfigured.value) {
      ElMessage.warning('请先在设置中配置阿里云 AccessKey')
      return
    }
    pushHistory()
    processing.value = true
    // 订阅云端流水线诊断日志，输出到应用 console（F12 可见）
    const { listen } = await import('@tauri-apps/api/event')
    const unlisten = await listen<string>('aliyun-log', (e) => {
      console.log(e.payload)
    })
    console.log('%c[云端抠图开始]', 'color:#409eff;font-weight:bold')
    try {
      syncImage(await removeBackgroundCloud(image.value))
      ElMessage.success('云端抠图完成')
    } catch (e: any) { ElMessage.error(`抠图失败：${e?.message || e}`) } finally {
      unlisten()
      processing.value = false
    }
    return
  }

  // 本地引擎分支（原逻辑）
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

// ── 智能裁剪 ──
async function handleTrim() {
  if (!image.value) return
  pushHistory(); processing.value = true
  try { syncImage(await smartCrop(image.value, { threshold: 0 })); ElMessage.success('已去除透明边距') }
  catch (e: any) { ElMessage.error(`去边距失败：${e?.message || e}`) } finally { processing.value = false }
}

async function handleCropAspect(w: number, h: number) {
  if (!image.value) return
  pushHistory(); processing.value = true
  try { syncImage(await smartCrop(image.value, { ratioW: w, ratioH: h })); ElMessage.success(`已裁剪为 ${w}:${h}`) }
  catch (e: any) { ElMessage.error(`裁剪失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── 边缘净化 ──
const edgeAmount = ref(2)       // erode/stroke/decontaminate 用（像素）
const featherAmount = ref(1.5)  // feather 用（sigma）
const strokeColor = ref('#000000')

async function handleEdgeRefine(op: 'erode' | 'feather' | 'decontaminate' | 'stroke') {
  if (!image.value) return
  const amount = op === 'feather' ? featherAmount.value : edgeAmount.value
  const color = op === 'stroke' ? hexToRgb(strokeColor.value) : undefined
  pushHistory(); processing.value = true
  try {
    syncImage(await edgeRefine(image.value, op, amount, color))
    const names = { erode: '收缩', feather: '羽化', decontaminate: '去色晕', stroke: '内描边' }
    ElMessage.success(`${names[op]}完成`)
  } catch (e: any) { ElMessage.error(`处理失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── 形状遮罩 ──
// shapeRatio 用「短边百分比」（0~50），0=直角，50%=正圆，与图片尺寸无关
const shapeRatio = ref(15)

// 比例 → 像素半径（按短边算），用于调后端
const shapeRadiusPx = computed(() => {
  const shortSide = Math.min(imgNatural.value.w, imgNatural.value.h)
  return Math.round((shapeRatio.value / 100) * shortSide)
})

// 实时预览：用 CSS clip-path 圆角裁切图片（形状遮罩激活时生效，拖滑块立即变化）
const shapeClipActive = computed(() => activeTool.value === 'shapeMask')
const shapePreviewPercent = computed(() => Math.min(shapeRatio.value, 50))
const shapeClipStyle = computed(() => {
  if (!shapeClipActive.value) return {}
  // clip-path 用百分比圆角，0% 直角 → 50% 正圆
  return { clipPath: `inset(0 round ${shapePreviewPercent.value}%)` }
})

// 形状遮罩应用：比例≥50 当圆形（裁内切正方形），否则圆角矩形
async function handleShapeMask() {
  if (!image.value) return
  const isCircle = shapeRatio.value >= 50
  pushHistory(); processing.value = true
  try {
    if (isCircle) {
      syncImage(await applyShapeMask(image.value, 'circle', 0))
      ElMessage.success('已应用圆形遮罩（自动裁内切正方形）')
    } else {
      syncImage(await applyShapeMask(image.value, 'rounded', shapeRadiusPx.value))
      ElMessage.success(`已应用圆角遮罩（${shapeRatio.value}%）`)
    }
  } catch (e: any) { ElMessage.error(`应用失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── 调色 ──
const adjBrightness = ref(0)
const adjContrast = ref(0)
const adjSaturation = ref(0)

async function handleAdjustColor() {
  if (!image.value) return
  pushHistory(); processing.value = true
  try {
    syncImage(await adjustColor(image.value, adjBrightness.value, adjContrast.value, adjSaturation.value))
    ElMessage.success('调色完成')
  } catch (e: any) { ElMessage.error(`调色失败：${e?.message || e}`) } finally { processing.value = false }
}

// ── computed ──
const imageTransform = computed(() => `translate(${panX.value}px, ${panY.value}px) scale(${scale.value})`)
</script>

<template>
  <div
    class="edit-root"
    :class="{ 'drag-active': dragOver && image }"
    @dragenter="onDragEnter"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <!-- 顶部栏（有图才显示） -->
    <div class="top-bar" v-if="image">
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
        <el-button size="small" type="primary" @click="router.push('/export')" :disabled="!image">去导出</el-button>
        <el-button size="small" @click="handleClose" :disabled="!image"><el-icon><Close /></el-icon> 关闭</el-button>
      </div>
    </div>

    <!-- 空状态：居中大入口 -->
    <div v-if="!image" class="empty-hero">
      <el-upload :show-file-list="false" :before-upload="openFile" accept="image/*" class="hero-upload">
        <div class="hero-card" :class="{ 'drag-hover': dragOver }">
          <el-icon :size="64" class="hero-icon"><UploadFilled /></el-icon>
          <div class="hero-title">打开图片</div>
          <div class="hero-hint">点击选择，或拖拽图片到此处</div>
        </div>
      </el-upload>
    </div>

    <!-- 编辑区 -->
    <div v-else class="editor-body">
      <!-- 左侧工具栏（PS 风格竖条） -->
      <div class="tool-rail">
        <div v-for="g in toolGroups" :key="g.label" class="tool-group">
          <div class="tool-group-label">{{ g.label }}</div>
          <el-tooltip v-for="t in g.items" :key="t.id" :content="t.name" placement="right">
            <button
              class="tool-btn"
              :class="{ active: activeTool === t.id }"
              :disabled="!image"
              @click="selectTool(t.id)"
            >
              <el-icon :size="20"><component :is="t.icon" /></el-icon>
            </button>
          </el-tooltip>
        </div>
      </div>

      <!-- 手动修补工具区（隐藏抽屉，控件铺在画布上方 + 画布左右分区） -->
      <div v-if="touchupActive" class="touchup-workspace">
        <!-- 顶部控件栏 -->
        <div class="touchup-toolbar">
          <div class="tt-mode">
            <button class="mode-btn" :class="{ active: touchupMode === 'remove', remove: true }" @click="touchupMode = 'remove'">
              <el-icon :size="18"><Close /></el-icon><span>去除</span>
            </button>
            <button class="mode-btn" :class="{ active: touchupMode === 'keep', keep: true }" @click="touchupMode = 'keep'">
              <el-icon :size="18"><Check /></el-icon><span>保留</span>
            </button>
          </div>
          <div class="tt-brush">
            <span class="tool-desc">画笔 {{ touchupBrushSize }}px</span>
            <el-slider v-model="touchupBrushSize" :min="2" :max="80" size="small" style="width:140px" />
          </div>
          <div class="tt-actions">
            <el-button size="small" @click="resetTouchup">重置</el-button>
            <el-button size="small" @click="invertTouchup">反选</el-button>
            <el-button size="small" type="primary" @click="applyTouchup" :loading="processing">应用</el-button>
            <el-button size="small" text @click="closeTool"><el-icon><Close /></el-icon></el-button>
          </div>
        </div>

        <!-- 画布：左右分区（左绘画 / 右预览） -->
        <div class="touchup-canvas-area">
          <!-- 左：绘画区 -->
          <div
            ref="canvasRef"
            class="canvas tt-paint"
            v-loading="processing"
            @wheel="onCanvasWheel"
          >
            <div class="canvas-bg checkerboard" />
            <img :src="toDataUrl(image)" class="canvas-img" :style="{ transform: imageTransform }" draggable="false" />
            <canvas
              ref="touchupCanvas"
              class="touchup-canvas"
              :style="{ transform: imageTransform, transformOrigin: '0 0' }"
              @mousedown="startTouchupStroke"
              @mousemove="continueTouchupStroke"
              @mouseup="endTouchupStroke"
              @mouseenter="brushCursor.visible = true"
              @mouseleave="() => { brushCursor.visible = false; endTouchupStroke() }"
              @wheel.prevent="onCanvasWheel"
            />
            <!-- 红色遮罩画布：覆盖在触摸画布上，半透明红 = 保留区（pointer-events:none 不挡绘制） -->
            <canvas
              ref="maskCanvas"
              class="mask-canvas"
              :style="{ transform: imageTransform, transformOrigin: '0 0' }"
            />
            <!-- 笔刷光标跟随圆（z-index 高于触摸 canvas） -->
            <div
              v-show="brushCursor.visible"
              class="brush-cursor"
              :class="touchupMode"
              :style="{
                left: brushCursor.x + 'px',
                top: brushCursor.y + 'px',
                width: touchupBrushSize * scale * 2 + 'px',
                height: touchupBrushSize * scale * 2 + 'px',
              }"
            />
            <div class="tt-label">绘画区</div>
          </div>

          <!-- 右：预览区 -->
          <div class="tt-preview checkerboard">
            <img v-if="previewUrl" :src="previewUrl" class="preview-img" draggable="false" />
            <div class="tt-label">预览区</div>
          </div>
        </div>
      </div>

      <!-- 画布（其他工具用） -->
      <div
        v-else
        ref="canvasRef"
        class="canvas"
        :class="{ 'canvas-eyedropper': colorActive }"
        v-loading="processing"
        @wheel="onCanvasWheel"
        @mousedown="onCanvasMouseDown"
        @mousemove="onCanvasMouseMove"
        @mouseup="onCanvasMouseUp"
        @mouseleave="onCanvasMouseUp"
      >
        <div class="canvas-bg checkerboard" />
        <img :src="toDataUrl(image)" class="canvas-img" :style="{ transform: imageTransform, ...shapeClipStyle }" draggable="false" />

        <!-- 形状遮罩九宫格辅助线（跟随图片 transform，帮看构图/圆角对称） -->
        <div v-if="shapeClipActive" class="shape-grid" :style="{ transform: imageTransform, transformOrigin: '0 0', width: imgNatural.w + 'px', height: imgNatural.h + 'px' }">
          <div class="shape-grid-h" style="top: 33.33%" />
          <div class="shape-grid-h" style="top: 66.66%" />
          <div class="shape-grid-v" style="left: 33.33%" />
          <div class="shape-grid-v" style="left: 66.66%" />
        </div>

        <!-- 形状遮罩 canvas 预览已移除，改用 CSS clip-path 直接作用于图片 -->
        <!-- 裁剪取景框（固定在画布中央，图片在背后缩放平移） -->
        <div v-if="cropActive" class="crop-overlay">
          <div class="crop-viewfinder" :style="cropBoxStyle">
            <div class="crop-grid-h" v-for="i in 2" :key="'h'+i" :style="{ top: `${(100/3)*i}%` }" />
            <div class="crop-grid-v" v-for="i in 2" :key="'v'+i" :style="{ left: `${(100/3)*i}%` }" />
          </div>
        </div>
      </div>
    </div>


    <!-- 右侧工具配置抽屉（点工具后滑出，不遮罩画布） -->
    <el-drawer
      v-model="drawerVisible"
      :modal="false"
      :modal-penetrable="true"
      :trap-focus="false"
      :with-header="false"
      direction="rtl"
      size="300px"
      :show-close="false"
    >
      <div class="drawer-body">
        <!-- 标题栏 -->
        <div class="drawer-header">
          <span>{{ activeTool ? toolName(activeTool) : '' }}</span>
          <el-button text @click="closeTool"><el-icon><Close /></el-icon></el-button>
        </div>

        <!-- 裁剪 -->
        <div v-if="activeTool === 'crop'" class="drawer-section">
          <div class="param">
            <span class="tool-desc">取景框：{{ Math.round(cropSize * 100) }}%</span>
            <el-slider v-model="cropSize" :min="0.3" :max="1.0" :step="0.05" size="small" />
          </div>
          <div class="btn-row">
            <el-button type="primary" @click="confirmCrop" style="flex:1">确认</el-button>
            <el-button @click="cancelCrop" style="flex:1">取消</el-button>
          </div>
          <p class="tool-desc">滚轮缩放，拖拽移动图片，方向键微调（Shift 加速）</p>
        </div>

        <!-- 去底色 -->
        <div v-else-if="activeTool === 'removeColor'" class="drawer-section">
          <div class="param">
            <span class="tool-desc">背景色（点画布拾取，或下方调整）</span>
            <el-color-picker v-model="bgColor" size="small" style="width:100%; margin-top:4px" />
          </div>
          <div class="param">
            <span class="tool-desc">容差：{{ colorTolerance }}</span>
            <el-slider v-model="colorTolerance" :min="0" :max="200" size="small" />
          </div>
          <div class="btn-row">
            <el-button type="primary" @click="applyRemoveColor" :loading="processing" style="flex:1">应用</el-button>
            <el-button @click="cancelRemoveColor" style="flex:1">取消</el-button>
          </div>
          <p class="tool-desc">直接在画布上点击要去除的颜色（吸管），适合白底图标</p>
        </div>

        <!-- 抠图 -->
        <div v-else-if="activeTool === 'removeBg'" class="drawer-section">
          <div class="engine-switch">
            <span class="tool-desc">抠图引擎</span>
            <el-radio-group :model-value="engine" size="small" style="width:100%; margin-top:4px" @change="onEngineChange">
              <el-radio-button value="local">本地模型</el-radio-button>
              <el-radio-button value="cloud">云端 阿里云</el-radio-button>
            </el-radio-group>
          </div>
          <template v-if="engine === 'local'">
            <div class="bg-model-picker" style="margin-top:8px">
              <span class="tool-desc">智能抠图模型</span>
              <el-select :model-value="currentBgModelId" size="small" style="width:100%; margin-top:4px" placeholder="无可用模型" @change="onBgModelChange">
                <el-option v-for="m in downloadedBgModels" :key="m.id" :value="m.id" :label="m.name" />
              </el-select>
              <p v-if="!downloadedBgModels.length" class="tool-desc" style="color: var(--el-color-warning)">
                尚未下载任何模型，点击下方按钮下载
              </p>
            </div>
            <el-button :disabled="processing || downloading || !downloadedBgModels.length" @click="handleRemoveBg" style="width:100%; margin-top:8px">
              <el-icon><MagicStick /></el-icon> 智能抠图
            </el-button>
            <el-progress v-if="downloading" :percentage="downloadPct" :stroke-width="6" style="margin-top:8px" />
            <p class="tool-desc">AI 识别物体（适合照片/复杂背景）</p>
          </template>
          <template v-else>
            <div class="bg-model-picker" style="margin-top:8px">
              <span class="tool-desc">云端模型</span>
              <el-select :model-value="currentCloudModel" size="small" style="width:100%; margin-top:4px" @change="onCloudModelChange">
                <el-option value="common" label="通用分割" />
                <el-option value="commodity" label="商品分割" />
              </el-select>
              <p class="tool-desc">商品分割对实拍/产品图标更佳</p>
            </div>
            <div class="cloud-status" style="margin-top:8px">
              <span v-if="cloudKeyConfigured" class="tool-desc" style="color: var(--el-color-success)">✓ 阿里云 AccessKey 已配置</span>
              <span v-else class="tool-desc" style="color: var(--el-color-warning)">
                未配置 AccessKey，
                <el-link type="primary" :underline="false" @click="goToSettings">前往设置</el-link>
              </span>
            </div>
            <el-button :disabled="processing || !cloudKeyConfigured" @click="handleRemoveBg" style="width:100%; margin-top:8px">
              <el-icon><MagicStick /></el-icon> 智能抠图（云端）
            </el-button>
            <p class="tool-desc">阿里云分割抠图，需联网，约 0.002 元/次</p>
          </template>
        </div>

        <!-- 智能裁剪 -->
        <div v-else-if="activeTool === 'smartCrop'" class="drawer-section">
          <el-button :disabled="processing || !image" @click="handleTrim" style="width:100%">
            去除透明边距
          </el-button>
          <p class="tool-desc">自动裁掉四周空白，主体贴边</p>
          <el-divider />
          <span class="tool-desc">按宽高比裁剪</span>
          <div class="btn-row" style="flex-wrap:wrap; margin-top:6px">
            <el-button size="small" :disabled="processing || !image" @click="handleCropAspect(1,1)">1:1</el-button>
            <el-button size="small" :disabled="processing || !image" @click="handleCropAspect(3,4)">3:4</el-button>
            <el-button size="small" :disabled="processing || !image" @click="handleCropAspect(4,3)">4:3</el-button>
          </div>
        </div>

        <!-- 边缘净化 -->
        <div v-else-if="activeTool === 'edgeRefine'" class="drawer-section">
          <div class="param">
            <span class="tool-desc">收缩/描边/去色晕 强度：{{ edgeAmount }}px</span>
            <el-slider v-model="edgeAmount" :min="1" :max="8" size="small" />
          </div>
          <div class="param">
            <span class="tool-desc">羽化半径：{{ featherAmount }}</span>
            <el-slider v-model="featherAmount" :min="0.5" :max="4" :step="0.1" size="small" />
          </div>
          <div class="param">
            <span class="tool-desc">内描边颜色</span>
            <el-color-picker v-model="strokeColor" size="small" style="width:100%; margin-top:4px" />
          </div>
          <el-divider />
          <div class="btn-row" style="flex-wrap:wrap">
            <el-button size="small" :disabled="processing || !image" @click="handleEdgeRefine('erode')">收缩</el-button>
            <el-button size="small" :disabled="processing || !image" @click="handleEdgeRefine('feather')">羽化</el-button>
            <el-button size="small" :disabled="processing || !image" @click="handleEdgeRefine('decontaminate')">去色晕</el-button>
            <el-button size="small" :disabled="processing || !image" @click="handleEdgeRefine('stroke')">内描边</el-button>
          </div>
        </div>

        <!-- 形状遮罩 -->
        <div v-else-if="activeTool === 'shapeMask'" class="drawer-section">
          <p class="tool-desc">拖动滑块实时预览圆角效果，画布上的图片会即时变化。</p>
          <div class="param">
            <span class="tool-desc">圆角：{{ shapeRatio }}%<span v-if="shapeRatio >= 50">（正圆）</span></span>
            <el-slider v-model="shapeRatio" :min="0" :max="50" size="small" />
          </div>
          <el-button type="primary" :disabled="processing || !image" @click="handleShapeMask" style="width:100%; margin-top:8px">
            <el-icon><Check /></el-icon> 应用遮罩
          </el-button>
          <p class="tool-desc">0% = 直角，50% = 正圆。正圆会自动裁成内切正方形。</p>
        </div>

        <!-- 调色 -->
        <div v-else-if="activeTool === 'adjustColor'" class="drawer-section">
          <div class="param">
            <span class="tool-desc">亮度：{{ adjBrightness }}</span>
            <el-slider v-model="adjBrightness" :min="-100" :max="100" size="small" />
          </div>
          <div class="param">
            <span class="tool-desc">对比度：{{ adjContrast }}</span>
            <el-slider v-model="adjContrast" :min="-100" :max="100" size="small" />
          </div>
          <div class="param">
            <span class="tool-desc">饱和度：{{ adjSaturation }}</span>
            <el-slider v-model="adjSaturation" :min="-100" :max="100" size="small" />
          </div>
          <el-button type="primary" :disabled="processing || !image" @click="handleAdjustColor" style="width:100%; margin-top:8px">
            应用调色
          </el-button>
        </div>
      </div>
    </el-drawer>

    <!-- 拖拽遮罩 -->
    <div v-if="dragOver && image" class="drop-overlay">
      <el-icon :size="48"><UploadFilled /></el-icon>
      <p>松开以打开图片</p>
    </div>
  </div>
</template>

<style scoped>
.edit-root { display: flex; flex-direction: column; height: calc(100vh - 110px); position: relative; }

/* 拖拽高亮 */
.edit-root.drag-active > *:not(.drop-overlay) { filter: brightness(0.6); }
.drop-overlay {
  position: absolute; inset: 0; z-index: 9999;
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  background: var(--el-color-primary-light-9); color: var(--el-color-primary);
  border: 3px dashed var(--el-color-primary); border-radius: 6px;
  pointer-events: none;
}
.drop-overlay p { margin-top: 12px; font-size: 18px; font-weight: 600; }

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

/* 空状态居中大入口 */
.empty-hero {
  flex: 1; display: flex; align-items: center; justify-content: center;
}
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

.editor-body { flex: 1; display: flex; gap: 12px; min-height: 0; }

/* 画布 */
.canvas {
  flex: 1; position: relative; overflow: hidden; border-radius: 6px;
  cursor: grab; min-width: 0;
}
.canvas:active { cursor: grabbing; }
.canvas-eyedropper, .canvas-eyedropper:active { cursor: crosshair; }
.canvas-bg { position: absolute; inset: 0; }

.canvas-img { position: absolute; top: 0; left: 0; transform-origin: 0 0; z-index: 2; }

/* 形状遮罩九宫格辅助线（叠加在图片上，跟随 transform） */
.shape-grid { position: absolute; top: 0; left: 0; pointer-events: none; z-index: 3; }
.shape-grid-h { position: absolute; left: 0; right: 0; border-top: 1px dashed rgba(255,255,255,0.7); }
.shape-grid-v { position: absolute; top: 0; bottom: 0; border-left: 1px dashed rgba(255,255,255,0.7); }

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
.touchup-canvas { position: absolute; top: 0; left: 0; pointer-events: auto; cursor: none; z-index: 8; }

/* 红色遮罩画布（叠在修补画布上方，标示保留区；不拦截鼠标事件） */
.mask-canvas { position: absolute; top: 0; left: 0; pointer-events: none; z-index: 9; }

/* 笔刷光标跟随圆（z-index 高于触摸 canvas，确保可见） */
.brush-cursor {
  position: absolute; pointer-events: none; z-index: 20;
  border-radius: 50%; transform: translate(-50%, -50%);
  border: 1.5px solid;
  box-shadow: 0 0 0 1px rgba(255,255,255,0.5);
  transition: border-color 0.15s, background 0.15s;
}
.brush-cursor.remove { border-color: var(--el-color-danger); background: rgba(245,108,108,0.18); }
.brush-cursor.keep   { border-color: var(--el-color-success); background: rgba(103,194,58,0.18); }

/* 手动修补工作区：顶部控件栏 + 下方左右分区的画布 */
.touchup-workspace {
  flex: 1; display: flex; flex-direction: column; gap: 8px; min-height: 0; min-width: 0;
}
.touchup-toolbar {
  display: flex; align-items: center; gap: 16px; flex-shrink: 0;
  padding: 6px 10px; border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px; background: var(--el-bg-color);
}
.tt-mode { display: flex; gap: 6px; }
.tt-brush { display: flex; align-items: center; gap: 8px; }
.tt-brush .tool-desc { margin: 0; white-space: nowrap; }
.tt-actions { display: flex; align-items: center; gap: 6px; margin-left: auto; }

/* 工具栏内紧凑模式按钮（图标+文字水平） */
.touchup-toolbar .mode-btn {
  display: inline-flex; align-items: center; gap: 4px;
  padding: 6px 14px; border-radius: 6px;
  border: 1px solid var(--el-border-color); background: var(--el-fill-color-blank);
  color: var(--el-text-color-regular); cursor: pointer;
  font-size: 13px; transition: all 0.15s;
}
.touchup-toolbar .mode-btn:hover { background: var(--el-fill-color-light); }
.touchup-toolbar .mode-btn.active.remove {
  border-color: var(--el-color-danger); color: var(--el-color-danger);
  background: var(--el-color-danger-light-9);
}
.touchup-toolbar .mode-btn.active.keep {
  border-color: var(--el-color-success); color: var(--el-color-success);
  background: var(--el-color-success-light-9);
}

/* 画布左右分区：左绘画 / 右预览 */
.touchup-canvas-area { flex: 1; display: flex; gap: 12px; min-height: 0; }
.tt-paint { flex: 1; min-width: 0; }
.tt-preview {
  flex: 1; min-width: 0; position: relative; border-radius: 6px; overflow: hidden;
  display: flex; align-items: center; justify-content: center; padding: 8px;
}
.tt-preview .preview-img {
  max-width: 100%; max-height: 100%; object-fit: contain;
  -webkit-user-drag: none; user-select: none;
}
/* 分区角标 */
.tt-label {
  position: absolute; top: 6px; left: 8px; z-index: 10;
  font-size: 11px; color: var(--el-text-color-secondary);
  background: rgba(0,0,0,0.35); color: #fff;
  padding: 2px 6px; border-radius: 3px; pointer-events: none;
}

/* 形状遮罩 canvas 预览（覆盖在图片上方，背景棋盘格透出来） */
.shape-canvas {
  position: absolute; top: 0; left: 0; pointer-events: none; z-index: 8;
  image-rendering: auto;
}

.zoom-label { font-size: 12px; color: var(--el-text-color-secondary); min-width: 36px; text-align: center; }

/* 左侧工具栏（PS 风格竖条） */
.tool-rail {
  width: 48px; flex-shrink: 0; display: flex; flex-direction: column;
  gap: 2px; padding: 6px 0; align-items: center;
}
.tool-group {
  display: flex; flex-direction: column; align-items: center; gap: 2px;
  width: 100%;
}
.tool-group-label {
  font-size: 9px; color: var(--el-text-color-placeholder);
  text-align: center; padding: 8px 0 4px; letter-spacing: 1px;
}
.tool-group + .tool-group { border-top: 1px solid var(--el-border-color-lighter); margin-top: 4px; }
.tool-btn {
  width: 40px; height: 40px; border-radius: 6px; border: 1px solid transparent;
  background: transparent; cursor: pointer; display: flex; align-items: center; justify-content: center;
  color: var(--el-text-color-regular); transition: all 0.15s;
}
.tool-btn:hover:not(:disabled) { background: var(--el-fill-color-light); }
.tool-btn.active {
  background: var(--el-color-primary-light-9);
  border-color: var(--el-color-primary);
  color: var(--el-color-primary);
}
.tool-btn:disabled { opacity: 0.4; cursor: not-allowed; }

.tool-desc { font-size: 12px; color: var(--el-text-color-secondary); margin: 6px 0 0; }
.btn-row { display: flex; gap: 6px; margin-top: 4px; }
.param { margin-bottom: 12px; }

/* Drawer 内容 */
.drawer-body { padding: 16px; }
.drawer-header {
  display: flex; align-items: center; justify-content: space-between;
  font-size: 15px; font-weight: 600; margin-bottom: 16px;
}
.drawer-section { display: flex; flex-direction: column; }
</style>
