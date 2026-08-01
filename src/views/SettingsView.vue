<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { openUrl } from '@tauri-apps/plugin-opener'
import type { FormInstance, FormRules } from 'element-plus'
import {
  listProviders,
  addProvider,
  updateProvider,
  deleteProvider,
  toggleProvider,
  reorderProviders,
  listBgModels,
  downloadBgModel,
  deleteBgModel,
  openModelLocation,
  getConfig,
  setConfig,
  type ProviderEntry,
  type ProviderUpsertRequest,
  type BgModelEntry,
} from '../api/client'

const providers = ref<ProviderEntry[]>([])
const appVersion = ref('')
const loading = ref(false)

// 抠图设置
const bgModels = ref<BgModelEntry[]>([])
const bgDownloading = ref('')
const bgDownPct = ref(0)

// 云端抠图（阿里云 VIAPI）AccessKey
const aliyunAk = ref('')
const aliyunSk = ref('')
const aliyunModel = ref<'common' | 'commodity'>('common')
const aliyunDialogVisible = ref(false)

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
  // 读应用版本号（来自 tauri.conf.json，不硬编码）
  try {
    const { getVersion } = await import('@tauri-apps/api/app')
    appVersion.value = await getVersion()
  } catch { appVersion.value = '' }
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

async function openDoc(url: string) {
  try {
    await openUrl(url)
  } catch (e: any) {
    ElMessage.error('打开文档失败：' + (e?.message || e))
  }
}

async function loadBgSettings() {
  try {
    bgModels.value = await listBgModels()
  } catch (e: any) {
    ElMessage.error('加载模型列表失败：' + (e?.message || e))
  }
  try {
    const cfg = await getConfig()
    aliyunAk.value = cfg.aliyun_ak ?? ''
    aliyunSk.value = cfg.aliyun_sk ?? ''
    aliyunModel.value = cfg.cloud_model === 'commodity' ? 'commodity' : 'common'
  } catch { /* 静默 */ }
}

async function saveAliyunKeys() {
  try {
    await setConfig('aliyun_ak', aliyunAk.value.trim())
    await setConfig('aliyun_sk', aliyunSk.value.trim())
    aliyunAk.value = aliyunAk.value.trim()
    aliyunSk.value = aliyunSk.value.trim()
    ElMessage.success('AccessKey 已保存')
    aliyunDialogVisible.value = false
  } catch (e: any) {
    ElMessage.error('保存失败：' + (e?.message || e))
  }
}

async function onAliyunModelChange(val: 'common' | 'commodity') {
  try {
    await setConfig('cloud_model', val)
    aliyunModel.value = val
    ElMessage.success(`默认模型已设为 ${val === 'commodity' ? '商品分割' : '通用分割'}`)
  } catch (e: any) {
    ElMessage.error('切换失败：' + (e?.message || e))
  }
}

async function refreshBgModels() {
  bgModels.value = await listBgModels()
}

async function selectModel(id: string) {
  const m = bgModels.value.find(x => x.id === id)
  if (!m) return
  if (!m.downloaded) {
    // 未下载：询问是否下载
    try {
      await ElMessageBox.confirm(
        `「${m.name}」尚未下载，是否现在下载？（${m.size}）`,
        '模型未下载',
        { confirmButtonText: '下载', cancelButtonText: '取消', type: 'info' }
      )
    } catch { return }
    await downloadModel(id)
    return
  }
  try {
    await setConfig('bg_model', id)
    await refreshBgModels()
    ElMessage.success(`已切换为 ${m.name}`)
  } catch (e: any) {
    ElMessage.error('切换失败：' + (e?.message || e))
  }
}

async function downloadModel(id: string) {
  const m = bgModels.value.find(x => x.id === id)
  if (!m) return
  bgDownloading.value = id; bgDownPct.value = 0
  try {
    // 下载即选用：同步当前模型
    await setConfig('bg_model', id)
    await downloadBgModel((pct: number) => { bgDownPct.value = Math.round(pct) })
    await refreshBgModels()
    ElMessage.success(`「${m.name}」下载完成，已自动启用`)
  } catch (e: any) { ElMessage.error('下载失败：' + (e?.message || e)) }
  finally { bgDownloading.value = '' }
}

async function importModel(id: string) {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const { invoke } = await import('@tauri-apps/api/core')
    const selected = await open({ filters: [{ name: 'ONNX 模型', extensions: ['onnx'] }], multiple: false })
    if (!selected) return
    await invoke('import_bg_model', { sourcePath: selected as string, modelId: id })
    await setConfig('bg_model', id)
    await refreshBgModels()
    ElMessage.success('模型已导入，已自动启用')
  } catch (e: any) { ElMessage.error('导入失败：' + (e?.message || e)) }
}

async function deleteModel(id: string) {
  const m = bgModels.value.find(x => x.id === id)
  if (!m) return
  try {
    await ElMessageBox.confirm(
      `确定删除已下载的「${m.name}」模型文件吗？此操作不可恢复。`,
      '删除确认',
      { type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消' }
    )
  } catch { return }
  try {
    await deleteBgModel(id)
    await refreshBgModels()
    ElMessage.success('已删除模型文件')
  } catch (e: any) {
    ElMessage.error('删除失败：' + (e?.message || e))
  }
}

async function openLocation(id: string) {
  try {
    await openModelLocation(id)
  } catch (e: any) {
    ElMessage.error('打开失败：' + (e?.message || e))
  }
}
</script>

<template>
  <div v-loading="loading">
    <h2 class="page-title">设置</h2>

    <el-tabs>
      <el-tab-pane label="生图服务" lazy>
        <div class="toolbar">
          <el-button type="primary" @click="openAdd">
            <el-icon><Plus /></el-icon> 新增服务商
          </el-button>
        </div>

        <div class="provider-list">
          <div
            v-for="(row, idx) in providers"
            :key="row.id"
            class="provider-row svc-card"
            :class="{ 'drag-src': dragIdx === idx, 'drop-target': dropIdx === idx && dragIdx !== idx }"
            @mousedown="onRowMouseDown(idx, $event)"
          >
            <div class="row-main">
              <div class="row-top">
                <span class="row-name">{{ row.name }}</span>
                <el-tag v-if="row.apiKey" size="small" type="success" effect="plain">已配置</el-tag>
                <el-tag v-else size="small" type="info" effect="plain">未配置</el-tag>
              </div>
              <div class="row-meta">
                <span class="row-model">{{ row.model || '默认' }}</span>
              </div>
            </div>
            <div class="row-actions">
              <el-switch :model-value="row.enabled" @change="handleToggle(row)" size="small" />
              <el-button text size="small" type="primary" @click="openEdit(row)">设置</el-button>
              <el-button text size="small" type="danger" @click="handleDelete(row)">删除</el-button>
            </div>
          </div>
        </div>
      </el-tab-pane>

      <el-tab-pane label="抠图服务" lazy>
        <h3 class="section-title">云端服务</h3>
        <div class="provider-list">
          <div class="svc-card provider-row">
            <div class="row-main">
              <div class="row-top">
                <span class="row-name">阿里云分割抠图</span>
                <el-tag v-if="aliyunAk && aliyunSk" size="small" type="success" effect="plain">已配置</el-tag>
                <el-tag v-else size="small" type="info" effect="plain">未配置</el-tag>
              </div>
              <div class="row-meta">
                <span class="row-model">{{ aliyunModel === 'commodity' ? '商品分割' : '通用分割' }} · 国内 · 约 0.002 元/次</span>
              </div>
            </div>
            <div class="row-actions">
              <el-button text size="small" type="primary" @click="aliyunDialogVisible = true">设置</el-button>
            </div>
          </div>
        </div>

        <el-divider />

        <h3 class="section-title">本地模型</h3>
        <div class="provider-list">
          <div v-for="m in bgModels" :key="m.id" class="svc-card provider-row bg-model-card" :class="{ selected: m.current }">
            <div class="row-main">
              <div class="row-top">
                <span class="row-name">{{ m.name }}</span>
                <el-tag v-if="m.current" type="success" size="small">使用中</el-tag>
                <el-tag v-else-if="m.downloaded" type="info" size="small">已下载</el-tag>
                <el-tag v-else type="warning" size="small" effect="plain">未下载</el-tag>
              </div>
              <div class="row-meta">
                <span class="row-model">大小：{{ m.size }}</span>
              </div>
              <el-progress v-if="bgDownloading === m.id" :percentage="bgDownPct" :stroke-width="6" style="margin-top: 6px" />
            </div>
            <div class="row-actions">
              <!-- 未下载：主操作下载 -->
              <template v-if="!m.downloaded">
                <el-button text size="small" type="primary" :loading="bgDownloading === m.id" @click="downloadModel(m.id)">下载</el-button>
                <el-button text size="small" @click="importModel(m.id)" title="导入本地 ONNX">导入</el-button>
              </template>
              <!-- 已下载但非当前：主操作选用 -->
              <template v-else-if="!m.current">
                <el-button text size="small" type="primary" @click="selectModel(m.id)">选用</el-button>
                <el-button text size="small" @click="importModel(m.id)" title="重新导入">导入</el-button>
                <el-button text size="small" @click="openLocation(m.id)" title="打开文件位置">位置</el-button>
                <el-button text size="small" type="danger" @click="deleteModel(m.id)">删除</el-button>
              </template>
              <!-- 当前使用 -->
              <template v-else>
                <el-button text size="small" @click="openLocation(m.id)" title="打开文件位置">位置</el-button>
                <el-button text size="small" type="danger" @click="deleteModel(m.id)">删除</el-button>
              </template>
            </div>
          </div>
        </div>
      </el-tab-pane>

      <el-tab-pane label="关于" lazy>
        <div class="about-page">
          <img src="/icon.png" class="about-logo" alt="IconForge" />
          <h2 class="about-name">IconForge</h2>
          <p v-if="appVersion" class="about-version">版本 {{ appVersion }}</p>
          <p class="about-desc">AI 图标生成与编辑桌面应用</p>

          <el-divider />

          <dl class="about-info">
            <div class="info-row">
              <dt>作者</dt>
              <dd>Silas</dd>
            </div>
            <div class="info-row">
              <dt>工作室</dt>
              <dd>奥哈悠工作室</dd>
            </div>
            <div class="info-row">
              <dt>邮箱</dt>
              <dd>
                <el-link type="primary" :underline="false" @click="openUrl('mailto:silas@890625.com')">
                  silas@890625.com
                </el-link>
              </dd>
            </div>
          </dl>

          <el-divider />

          <p class="about-tech">Tauri 2.x · Vue 3 · Element Plus</p>
          <p class="about-copy">Copyright © 2026 奥哈悠工作室（Silas）</p>
        </div>
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

    <!-- 阿里云抠图设置对话框 -->
    <el-dialog
      v-model="aliyunDialogVisible"
      title="阿里云分割抠图 设置"
      width="520px"
      :close-on-click-modal="false"
    >
      <p class="tool-desc">
        阿里云视觉智能 VIAPI，约 0.002 元/次。需 RAM 用户并授予 AliyunVIAPIFullAccess 权限。
        <el-link type="primary" :underline="false" @click.prevent="openDoc('https://help.aliyun.com/zh/viapi/developer-reference/api-k8cs8t')">查看文档</el-link>
      </p>
      <el-form label-position="top">
        <el-form-item label="AccessKey ID">
          <el-input v-model="aliyunAk" placeholder="如 LTAI..." size="small" />
        </el-form-item>
        <el-form-item label="AccessKey Secret">
          <el-input v-model="aliyunSk" type="password" show-password placeholder="AccessKey Secret" size="small" />
        </el-form-item>
        <el-form-item label="默认模型">
          <el-select :model-value="aliyunModel" size="small" style="width:100%" @change="onAliyunModelChange">
            <el-option value="common" label="通用分割" />
            <el-option value="commodity" label="商品分割" />
          </el-select>
          <span class="tool-desc">商品分割对实拍/产品图标更佳，不适合卡通图；编辑页可临时切换</span>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="aliyunDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="saveAliyunKeys">保存</el-button>
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

/* 三套卡片统一基准 */
.svc-card {
  padding: 12px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  background: var(--el-bg-color);
}

/* 服务商列表（生图 / 阿里云） */
.provider-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.provider-row {
  display: flex;
  align-items: center;
  gap: 12px;
  cursor: grab;
  user-select: none;
}

.provider-row:active { cursor: grabbing; }

.provider-row.drag-src { opacity: 0.3; }

.provider-row.drop-target {
  border-color: var(--el-color-primary);
  border-style: dashed;
}


.row-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.row-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.row-meta {
  display: flex;
  align-items: center;
  gap: 8px;
}

.row-name {
  font-weight: 600;
  font-size: 14px;
}

.row-model {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.row-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  align-self: center;
  flex-shrink: 0;
}
/* 干掉 Element Plus 给相邻按钮加的默认 margin-left，统一用 gap 控制间距 */
.row-actions .el-button + .el-button { margin-left: 0; }

.text-muted {
  color: var(--el-text-color-secondary);
}

/* 本地模型（与生图/阿里云共用 .provider-list / .svc-card / .provider-row） */
.bg-model-card.selected { border-color: var(--el-color-primary); }

/* 抠图 tab 分组标题 */
.section-title {
  margin: 0 0 12px;
  font-size: 15px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

/* 弹窗内说明文字 */
.tool-desc { font-size: 12px; color: var(--el-text-color-secondary); margin: 6px 0; line-height: 1.5; }

/* 关于页 */
.about-page { max-width: 420px; margin: 0 auto; text-align: center; padding: 24px 0; }
.about-logo { width: 96px; height: 96px; margin-bottom: 12px; }
.about-name { margin: 0; font-size: 24px; }
.about-version { margin: 4px 0 0; color: var(--el-text-color-secondary); font-size: 13px; }
.about-desc { margin: 8px 0 0; color: var(--el-text-color-regular); }
.about-info { margin: 0; text-align: left; }
.about-info .info-row { display: flex; align-items: center; padding: 6px 0; }
.about-info dt { width: 70px; color: var(--el-text-color-secondary); font-size: 13px; flex-shrink: 0; }
.about-info dd { margin: 0; font-size: 14px; }
.about-tech { color: var(--el-text-color-secondary); font-size: 12px; margin: 0; }
.about-copy { color: var(--el-text-color-placeholder); font-size: 12px; margin: 8px 0 0; }
</style>
