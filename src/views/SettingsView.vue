<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import type { FormInstance, FormRules } from 'element-plus'
import {
  listProviders,
  addProvider,
  updateProvider,
  deleteProvider,
  toggleProvider,
  reorderProviders,
  checkBgModel,
  downloadBgModel,
  setConfig,
  getConfig,
  type ProviderEntry,
  type ProviderUpsertRequest,
} from '../api/client'

const providers = ref<ProviderEntry[]>([])
const loading = ref(false)

// 抠图设置
const hasModel = ref(false)
const modelDownPct = ref(0)
const modelDownloading = ref(false)
const bgThreshold = ref(0.5)

// 编辑对话框
const dialogVisible = ref(false)
const dialogTitle = ref('新增服务商')
const formRef = ref<FormInstance>()
const saving = ref(false)

const form = ref<ProviderUpsertRequest>({
  name: '',
  notes: '',
  website: '',
  apiKey: '',
  endpoint: '',
  model: '',
})
const editingId = ref<string | null>(null)

const rules: FormRules = {
  name: [{ required: true, message: '请输入服务商名称', trigger: 'blur' }],
  endpoint: [{ required: true, message: '请输入请求地址', trigger: 'blur' }],
  apiKey: [{ required: true, message: '请输入 API Key', trigger: 'blur' }],
}

onMounted(async () => {
  await load()
  await loadBgSettings()
})

async function load() {
  loading.value = true
  try {
    providers.value = await listProviders()
    console.log('[设置] 加载服务商:', providers.value.length)
  } catch (e: any) {
    console.error('[设置] 加载失败:', e)
    ElMessage.error('加载服务商列表失败: ' + (typeof e === 'string' ? e : e?.message || JSON.stringify(e)))
  } finally {
    loading.value = false
  }
}

// ── 拖拽排序 ──
const dragIdx = ref(-1)
const dropIdx = ref(-1)
const floatClone = ref<HTMLElement | null>(null)

function onRowMouseDown(idx: number, e: MouseEvent) {
  const target = e.target as HTMLElement
  if (target.closest('button, .el-switch, .el-button, input')) return
  e.preventDefault()
  dragIdx.value = idx; dropIdx.value = idx

  // 克隆整行作为浮动 ghost
  const row = (e.currentTarget as HTMLElement).closest('.provider-row')!
  const rect = row.getBoundingClientRect()
  const clone = row.cloneNode(true) as HTMLElement
  clone.style.position = 'fixed'
  clone.style.left = rect.left + 'px'
  clone.style.top = rect.top + 'px'
  clone.style.width = rect.width + 'px'
  clone.style.zIndex = '9999'
  clone.style.pointerEvents = 'none'
  clone.style.boxShadow = '0 4px 16px rgba(0,0,0,0.25)'
  clone.style.opacity = '0.9'
  clone.style.transform = 'scale(1.02)'
  clone.classList.add('float-clone')
  document.body.appendChild(clone)
  floatClone.value = clone

  document.addEventListener('mousemove', onRowMouseMove)
  document.addEventListener('mouseup', onRowMouseUp)
}

function onRowMouseMove(e: MouseEvent) {
  const clone = floatClone.value; if (!clone) return
  const rect = clone.getBoundingClientRect()
  clone.style.left = (e.clientX - rect.width / 2) + 'px'
  clone.style.top = (e.clientY - rect.height / 2) + 'px'

  const rows = document.querySelectorAll('.provider-row')
  let nearest = dragIdx.value, minDist = Infinity
  rows.forEach((row, i) => {
    const r = row.getBoundingClientRect()
    const dist = Math.abs(e.clientY - (r.top + r.height / 2))
    if (dist < minDist) { minDist = dist; nearest = i }
  })
  dropIdx.value = nearest
}

async function onRowMouseUp() {
  document.removeEventListener('mousemove', onRowMouseMove)
  document.removeEventListener('mouseup', onRowMouseUp)
  if (floatClone.value) { floatClone.value.remove(); floatClone.value = null }
  if (dragIdx.value >= 0 && dropIdx.value >= 0 && dragIdx.value !== dropIdx.value) {
    const list = [...providers.value]
    const [item] = list.splice(dragIdx.value, 1)
    list.splice(dropIdx.value, 0, item)
    providers.value = list
    await reorderProviders(list.map(p => p.id))
  }
  dragIdx.value = -1; dropIdx.value = -1
}

function openAdd() {
  editingId.value = null
  dialogTitle.value = '新增服务商'
  form.value = { name: '', notes: '', website: '', apiKey: '', endpoint: '', model: '' }
  dialogVisible.value = true
}

function openEdit(row: ProviderEntry) {
  editingId.value = row.id
  dialogTitle.value = '编辑服务商'
  form.value = {
    name: row.name,
    notes: row.notes,
    website: row.website,
    apiKey: row.apiKey,
    endpoint: row.endpoint,
    model: row.model,
  }
  dialogVisible.value = true
}

async function handleSave() {
  const valid = await formRef.value?.validate().catch(() => false)
  if (!valid) return
  saving.value = true
  try {
    if (editingId.value) {
      await updateProvider(editingId.value, form.value)
      ElMessage.success('已更新')
    } else {
      await addProvider(form.value)
      ElMessage.success('已添加')
    }
    dialogVisible.value = false
    await load()
  } catch (e: any) {
    ElMessage.error('操作失败：' + (typeof e === 'string' ? e : JSON.stringify(e)))
  } finally {
    saving.value = false
  }
}

async function handleDelete(row: ProviderEntry) {
  try {
    await ElMessageBox.confirm(`确定删除「${row.name}」吗？`, '删除确认', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    })
  } catch {
    return
  }
  try {
    await deleteProvider(row.id)
    providers.value = providers.value.filter((p) => p.id !== row.id)
    ElMessage.success('已删除')
  } catch (e: any) {
    ElMessage.error(e?.message || '删除失败')
  }
}

async function handleToggle(row: ProviderEntry) {
  try {
    await toggleProvider(row.id, !row.enabled)
    row.enabled = !row.enabled
  } catch (e: any) {
    ElMessage.error(e?.message || '操作失败')
  }
}

async function loadBgSettings() {
  try {
    hasModel.value = await checkBgModel()
  } catch { /* ignore */ }
  try {
    const cfg = await getConfig()
    if (cfg?.bg_threshold) {
      bgThreshold.value = parseFloat(cfg.bg_threshold)
    }
  } catch { /* use default */ }
}

async function handleImportModel() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const { invoke } = await import('@tauri-apps/api/core')
    const selected = await open({
      filters: [{ name: 'ONNX 模型', extensions: ['onnx'] }],
      multiple: false,
    })
    if (!selected) return
    const path = typeof selected === 'string' ? selected : selected.path
    await invoke('import_bg_model', { sourcePath: path })
    hasModel.value = true
    ElMessage.success('模型已导入')
  } catch (e: any) {
    ElMessage.error(`导入失败：${e?.message || e}`)
  }
}

async function handleDownloadModel() {
  modelDownloading.value = true
  modelDownPct.value = 0
  try {
    await downloadBgModel((pct) => {
      modelDownPct.value = Math.round(pct)
    })
    hasModel.value = true
    ElMessage.success('模型下载完成')
  } catch (e: any) {
    ElMessage.error(`下载失败：${e?.message || e}`)
  } finally {
    modelDownloading.value = false
  }
}

async function handleThresholdChange(val: number) {
  try {
    await setConfig('bg_threshold', String(val))
  } catch { /* ignore */ }
}
</script>

<template>
  <div v-loading="loading">
    <h2 class="page-title">设置</h2>

    <el-tabs>
      <el-tab-pane label="服务商" lazy>
        <div class="toolbar">
          <el-button type="primary" @click="openAdd">
            <el-icon><Plus /></el-icon> 新增服务商
          </el-button>
        </div>

        <div class="provider-list">
          <div
            v-for="(row, idx) in providers"
            :key="row.id"
            class="provider-row"
            :class="{ 'drag-src': dragIdx === idx, 'drop-target': dropIdx === idx && dragIdx !== idx }"
            @mousedown="onRowMouseDown(idx, $event)"
          >
            <div class="row-info">
              <span class="row-name">{{ row.name }}</span>
              <el-tag size="small" type="warning" effect="plain">{{ row.model || '默认' }}</el-tag>
              <el-tag v-if="row.apiKey" size="small" type="success" effect="plain">已配置</el-tag>
              <el-tag v-else size="small" type="info" effect="plain">未配置</el-tag>
            </div>
            <el-switch :model-value="row.enabled" @change="handleToggle(row)" size="small" />
            <div class="row-actions">
              <el-button text size="small" type="primary" @click="openEdit(row)">编辑</el-button>
              <el-button text size="small" type="danger" @click="handleDelete(row)">删除</el-button>
            </div>
          </div>
        </div>
      </el-tab-pane>

      <el-tab-pane label="抠图" lazy>
        <el-card>
          <template #header>抠图模型</template>
          <el-descriptions :column="1" border>
            <el-descriptions-item label="模型状态">
              <el-tag v-if="hasModel" type="success" size="small">已下载</el-tag>
              <el-tag v-else type="info" size="small">未下载</el-tag>
            </el-descriptions-item>
            <el-descriptions-item label="模型文件">
              RMBG-2.0 ONNX
            </el-descriptions-item>
            <el-descriptions-item label="管理">
              <el-button
                size="small"
                :type="hasModel ? 'default' : 'primary'"
                :loading="modelDownloading"
                @click="handleDownloadModel"
              >
                {{ hasModel ? '重新下载' : '在线下载' }}
              </el-button>
              <el-button size="small" style="margin-left: 8px" @click="handleImportModel">
                导入本地模型
              </el-button>
              <el-progress
                v-if="modelDownloading"
                :percentage="modelDownPct"
                style="margin-top: 8px"
              />
            </el-descriptions-item>
          </el-descriptions>
        </el-card>

        <el-card style="margin-top: 16px">
          <template #header>抠图参数</template>
          <el-form label-position="top">
            <el-form-item>
              <template #label>
                抠图阈值：{{ bgThreshold.toFixed(2) }}
                <span class="text-muted" style="font-weight: normal; font-size: 12px">
                  — 越高越激进，抠掉更多边缘
                </span>
              </template>
              <el-slider
                v-model="bgThreshold"
                :min="0.1"
                :max="0.9"
                :step="0.05"
                show-stops
                style="max-width: 400px"
                @change="handleThresholdChange"
              />
            </el-form-item>
          </el-form>
        </el-card>
      </el-tab-pane>

      <el-tab-pane label="关于" lazy>
        <el-card>
          <p>IconForge — AI 图标生成桌面应用</p>
          <p>Tauri 2.x + Vue 3 + Element Plus</p>
        </el-card>
      </el-tab-pane>
    </el-tabs>

    <!-- 新增/编辑对话框 -->
    <el-dialog
      v-model="dialogVisible"
      :title="dialogTitle"
      width="520px"
      :close-on-click-modal="false"
    >
      <el-form ref="formRef" :model="form" :rules="rules" label-position="top">
        <el-form-item label="名称" prop="name">
          <el-input v-model="form.name" placeholder="例如：通义万相" />
        </el-form-item>
        <el-form-item label="备注" prop="notes">
          <el-input v-model="form.notes" placeholder="例如：公司专用账号" />
        </el-form-item>
        <el-form-item label="官网链接" prop="website">
          <el-input v-model="form.website" placeholder="https://example.com（可选）" />
        </el-form-item>
        <el-form-item label="API Key" prop="apiKey">
          <el-input v-model="form.apiKey" type="password" show-password placeholder="sk-..." />
        </el-form-item>
        <el-form-item label="模型" prop="model">
          <el-input v-model="form.model" placeholder="例如：qwen-image-2.0-pro" />
        </el-form-item>
        <el-form-item label="请求地址" prop="endpoint">
          <el-input v-model="form.endpoint" placeholder="https://api.example.com/v1/images/generations" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="saving" @click="handleSave">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.page-title {
  margin: 0 0 16px;
  font-size: 22px;
}

.toolbar {
  margin-bottom: 16px;
}

/* 服务商列表 */
.provider-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.provider-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  background: var(--el-bg-color);
  cursor: grab;
  user-select: none;
}

.provider-row:active { cursor: grabbing; }

.provider-row.drag-src { opacity: 0.3; }

.provider-row.drop-target {
  border-color: var(--el-color-primary);
  border-style: dashed;
}


.row-info {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.row-name {
  font-weight: 600;
  font-size: 14px;
}

.row-actions {
  display: flex;
  gap: 0;
}

.text-muted {
  color: var(--el-text-color-secondary);
}
</style>
