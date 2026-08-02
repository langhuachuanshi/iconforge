<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { useRouter } from 'vue-router'
import {
  convertFileSrc,
  deleteIcon,
  exportIconsToDir,
  fetchIconBase64,
  getIconPath,
  listIconVersions,
  loadIconVersion,
  listIcons,
  type IconMeta,
} from '../api/client'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { useWorkspaceStore } from '../stores/workspace'
import { copyText } from '../utils/clipboard'

const router = useRouter()
const workspace = useWorkspaceStore()

const icons = ref<IconMeta[]>([])
const loading = ref(false)
const total = ref(0)           // 图标总数（用于分页判断是否到底）
const pageSize = 30            // 每页条数
const hasMore = computed(() => icons.value.length < total.value)
const loadingMore = ref(false)

// ── 多选 ──
const selected = ref<Set<string>>(new Set())
const selectMode = ref(false) // 是否开启选择模式

const selectedCount = computed(() => selected.value.size)
const allSelected = computed(() => icons.value.length > 0 && selected.value.size === icons.value.length)

function toggleSelect(id: string) {
  const s = new Set(selected.value)
  if (s.has(id)) s.delete(id); else s.add(id)
  selected.value = s
}
function selectAll() {
  selected.value = new Set(icons.value.map((i) => i.id))
}
function invertSelect() {
  const s = new Set<string>()
  for (const i of icons.value) if (!selected.value.has(i.id)) s.add(i.id)
  selected.value = s
}
function clearSelect() {
  selected.value = new Set()
}

// ── 提示词查看（dialog）──
const promptDialog = ref<{ visible: boolean; concept: string; prompt: string }>({
  visible: false,
  concept: '',
  prompt: '',
})
function openPromptDialog(icon: IconMeta) {
  promptDialog.value = { visible: true, concept: icon.concept || '(未命名)', prompt: icon.prompt }
}
function toggleSelectMode() {
  selectMode.value = !selectMode.value
  if (!selectMode.value) clearSelect()
}

// keep-alive 缓存后用 onActivated：每次切回历史页都重新拉第一页，
// 这样在生成页新增图标后切回来能立即看到。现在只拉元数据（文本，毫秒级），
// 缩略图走 convertFileSrc 由 webview 原生懒加载，不再卡顿。
onActivated(async () => {
  await loadIcons()
})

async function loadIcons() {
  loading.value = true
  try {
    const result = await listIcons(pageSize, 0)
    icons.value = result.icons
    total.value = result.count
  } catch {
    ElMessage.error('加载历史记录失败')
  } finally {
    loading.value = false
  }
}

/** 加载下一页（滚动触底时调用） */
async function loadMore() {
  if (loadingMore.value || !hasMore.value) return
  loadingMore.value = true
  try {
    const result = await listIcons(pageSize, icons.value.length)
    icons.value.push(...result.icons)
    total.value = result.count
  } catch {
    ElMessage.error('加载更多失败')
  } finally {
    loadingMore.value = false
  }
}

// ── 触底分页：IntersectionObserver 监听 sentinel ──
const sentinelRef = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | null = null

function setupObserver() {
  teardownObserver()
  const el = sentinelRef.value
  if (!el) return
  observer = new IntersectionObserver((entries) => {
    if (entries[0]?.isIntersecting) loadMore()
  }, { rootMargin: '200px' })
  observer.observe(el)
}
function teardownObserver() {
  observer?.disconnect()
  observer = null
}
// sentinel 出现/重置第一页时重新挂载
watch(sentinelRef, (el) => { if (el) setupObserver() })
onBeforeUnmount(teardownObserver)

/** 载入到工作区并跳转编辑页：优先载入最新编辑版本，无版本则载原图 */
async function handleReuse(icon: IconMeta) {
  try {
    let base64: string
    const versions = await listIconVersions(icon.id)
    if (versions.length > 0) {
      // 有编辑版本 → 载入最新一条
      base64 = await loadIconVersion(versions[0].id)
    } else {
      // 无版本 → 载入原图
      base64 = await fetchIconBase64(icon.id)
    }
    workspace.setImage(base64, icon.id)
    ElMessage.success(versions.length > 0 ? `已载入最新版本（v${versions[0].versionNo}），跳转编辑页` : '已载入，跳转编辑页')
    router.push('/edit')
  } catch {
    ElMessage.error('载入失败')
  }
}

async function handleDelete(icon: IconMeta) {
  try {
    await ElMessageBox.confirm(
      `确定删除这张「${icon.concept || '图标'}」吗？`,
      '删除确认',
      { confirmButtonText: '删除', cancelButtonText: '取消', type: 'warning' }
    )
  } catch {
    return // 用户取消
  }

  try {
    await deleteIcon(icon.id)
    icons.value = icons.value.filter((i) => i.id !== icon.id)
    total.value = Math.max(0, total.value - 1)
    ElMessage.success('已删除')
  } catch {
    ElMessage.error('删除失败')
  }
}

/** 在系统资源管理器中定位图标文件 */
async function handleReveal(icon: IconMeta) {
  try {
    const path = await getIconPath(icon.id)
    await revealItemInDir(path)
  } catch {
    ElMessage.error('打开文件夹失败')
  }
}

// ── 批量删除 ──
async function handleBatchDelete() {
  if (selectedCount.value === 0) return
  try {
    await ElMessageBox.confirm(
      `确定删除选中的 ${selectedCount.value} 张图标吗？此操作不可恢复。`,
      '批量删除确认',
      { confirmButtonText: `删除 ${selectedCount.value} 张`, cancelButtonText: '取消', type: 'warning' }
    )
  } catch {
    return
  }
  let ok = 0, fail = 0
  for (const id of selected.value) {
    try { await deleteIcon(id); ok++ } catch { fail++ }
  }
  icons.value = icons.value.filter((i) => !selected.value.has(i.id))
  total.value = Math.max(0, total.value - ok)
  clearSelect()
  ElMessage.success(`已删除 ${ok} 张${fail > 0 ? `（${fail} 张失败）` : ''}`)
}

// ── 批量导出 ──
const exporting = ref(false)
async function handleBatchExport() {
  if (selectedCount.value === 0) return
  exporting.value = true
  try {
    const ids = Array.from(selected.value)
    const okCount = await exportIconsToDir(ids)
    if (okCount > 0) {
      ElMessage.success(`已导出 ${okCount} 个图标（每个一个 ZIP）`)
    } else {
      ElMessage.info('已取消')
    }
  } catch (e: any) {
    ElMessage.error('导出失败：' + (e?.message || e))
  } finally {
    exporting.value = false
  }
}

// ── 批量重生成（部分重现：取 concept + style 跳生成页）──
async function handleBatchRegen() {
  if (selectedCount.value === 0) return
  const picked = icons.value.filter((i) => selected.value.has(i.id))
  // 取第一个作为主参数填入生成页（concept/style），其余复制 concept 列表到剪贴板供用户参考
  const first = picked[0]
  const concepts = picked.map((i) => i.concept).filter(Boolean)
  // 通过 query 传递，生成页 onMounted 读取
  await router.push({ path: '/generate', query: {
    concept: first.concept,
    style: first.style,
    concepts: concepts.join('\n'),
  } })
  clearSelect()
}
</script>

<template>
  <div v-loading="loading">
    <div class="header-row">
      <h2 class="page-title">历史记录</h2>
      <div class="header-actions">
        <el-button :type="selectMode ? 'primary' : 'default'" @click="toggleSelectMode">
          <el-icon><Select /></el-icon> {{ selectMode ? '退出选择' : '批量操作' }}
        </el-button>
        <el-button text @click="loadIcons" :loading="loading">
          <el-icon><Refresh /></el-icon> 刷新
        </el-button>
      </div>
    </div>

    <!-- 批量操作工具栏 -->
    <div v-if="selectMode" class="batch-bar">
      <div class="batch-left">
        <el-checkbox :model-value="allSelected" @change="allSelected ? clearSelect() : selectAll()">
          全选
        </el-checkbox>
        <el-button text size="small" @click="invertSelect">反选</el-button>
        <span class="batch-count">已选 {{ selectedCount }} / {{ icons.length }}</span>
      </div>
      <div class="batch-right">
        <el-button size="small" :disabled="selectedCount === 0" @click="handleBatchRegen">
          <el-icon><RefreshRight /></el-icon> 重生成
        </el-button>
        <el-button size="small" :disabled="selectedCount === 0" :loading="exporting" @click="handleBatchExport">
          <el-icon><Download /></el-icon> 导出
        </el-button>
        <el-button size="small" type="danger" plain :disabled="selectedCount === 0" @click="handleBatchDelete">
          <el-icon><Delete /></el-icon> 删除
        </el-button>
      </div>
    </div>

    <el-empty
      v-if="!loading && icons.length === 0"
      description="还没有生成过图标，去生成第一张吧"
    >
      <el-button type="primary" @click="router.push('/generate')">
        去生成
      </el-button>
    </el-empty>

    <div v-else class="icon-grid">
      <el-card
        v-for="icon in icons"
        :key="icon.id"
        class="icon-card"
        :class="{ selected: selected.has(icon.id) }"
        :body-style="{ padding: '0' }"
        shadow="hover"
        @click="selectMode && toggleSelect(icon.id)"
      >
        <!-- 选择模式下的勾选角标 -->
        <div v-if="selectMode" class="select-badge" :class="{ checked: selected.has(icon.id) }">
          <el-icon v-if="selected.has(icon.id)"><Check /></el-icon>
        </div>
        <div class="icon-thumb checkerboard">
          <img
            v-if="icon.path"
            :src="convertFileSrc(icon.path)"
            :alt="icon.concept"
            loading="lazy"
          />
          <el-icon v-else :size="32"><Picture /></el-icon>
        </div>
        <div class="icon-info">
          <div class="info-concept" :title="icon.concept">
            {{ icon.concept || '(未命名)' }}
          </div>
          <div v-if="!selectMode" class="info-actions">
            <el-button size="small" type="primary" class="action-main" @click.stop="handleReuse(icon)">
              载入编辑
            </el-button>
            <el-tooltip v-if="icon.prompt" content="提示词" placement="top">
              <el-button size="small" @click.stop="openPromptDialog(icon)">
                <el-icon><Document /></el-icon>
              </el-button>
            </el-tooltip>
            <el-tooltip content="打开文件夹" placement="top">
              <el-button size="small" @click.stop="handleReveal(icon)">
                <el-icon><FolderOpened /></el-icon>
              </el-button>
            </el-tooltip>
            <el-tooltip content="删除" placement="top">
              <el-button size="small" type="danger" plain @click.stop="handleDelete(icon)">
                <el-icon><Delete /></el-icon>
              </el-button>
            </el-tooltip>
          </div>
        </div>
      </el-card>
    </div>

    <!-- 触底加载更多 -->
    <div
      v-if="icons.length > 0"
      ref="sentinelRef"
      class="load-more"
    >
      <span v-if="loadingMore">加载中…</span>
      <span v-else-if="!hasMore" class="no-more">共 {{ total }} 张</span>
    </div>

    <!-- 提示词查看弹窗 -->
    <el-dialog
      v-model="promptDialog.visible"
      :title="`提示词 · ${promptDialog.concept}`"
      width="600px"
      append-to-body
    >
      <div class="dialog-prompt-body">{{ promptDialog.prompt }}</div>
      <template #footer>
        <el-button @click="promptDialog.visible = false">关闭</el-button>
        <el-button type="primary" @click="copyText(promptDialog.prompt, '已复制提示词')">
          <el-icon><CopyDocument /></el-icon>&nbsp;复制
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.page-title {
  margin: 0;
  font-size: 22px;
}

/* 批量操作工具栏 */
.batch-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  margin-bottom: 12px;
  background: var(--el-color-primary-light-9);
  border: 1px solid var(--el-color-primary-light-7);
  border-radius: 6px;
}
.batch-left { display: flex; align-items: center; gap: 12px; }
.batch-right { display: flex; align-items: center; gap: 6px; }
.batch-count { font-size: 13px; color: var(--el-text-color-secondary); }

.icon-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 16px;
}

.icon-card {
  overflow: hidden;
  position: relative;
  cursor: default;
}
.icon-card.selected { border-color: var(--el-color-primary); box-shadow: 0 0 0 2px var(--el-color-primary-light-7); }

/* 选择角标 */
.select-badge {
  position: absolute;
  top: 8px;
  left: 8px;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 2px solid var(--el-border-color);
  background: rgba(255,255,255,0.9);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2;
  color: #fff;
}
.select-badge.checked {
  background: var(--el-color-primary);
  border-color: var(--el-color-primary);
}

.icon-thumb {
  width: 100%;
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}

.icon-thumb img {
  max-width: 80%;
  max-height: 80%;
  object-fit: contain;
}

.icon-info {
  padding: 12px;
}

.info-concept {
  font-weight: 600;
  font-size: 13px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-bottom: 10px;
  text-align: center;
}

/* 提示词 dialog 正文 */
.dialog-prompt-body {
  white-space: pre-wrap;
  word-break: break-word;
  font-family: ui-monospace, monospace;
  font-size: 13px;
  line-height: 1.6;
  color: var(--el-text-color-regular);
  background: var(--el-fill-color-light);
  padding: 12px;
  border-radius: 4px;
  max-height: 50vh;
  overflow-y: auto;
}

.info-actions {
  display: flex;
  flex-wrap: nowrap;
  gap: 8px;
  align-items: center;
}
.action-main { flex: 1; }

/* 触底加载更多 */
.load-more {
  padding: 24px 0 8px;
  text-align: center;
  font-size: 13px;
  color: var(--el-text-color-secondary);
}
.load-more .no-more { opacity: 0.7; }
</style>
