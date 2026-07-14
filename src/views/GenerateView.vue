<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { useRouter } from 'vue-router'
import {
  generateIcon,
  getProviders,
  getTemplates,
  toDataUrl,
  type ProviderInfo,
  type Template,
} from '../api/client'
import { useWorkspaceStore } from '../stores/workspace'

const router = useRouter()
const workspace = useWorkspaceStore()

const providers = ref<ProviderInfo[]>([])
const templates = ref<Template[]>([])
const loadingMeta = ref(false)

const concept = ref('')
const selectedStyle = ref('flat-design')
const selectedProvider = ref('')
const selectedSize = ref('1024x1024')
const bgColor = ref('')
const details = ref('')
const extraPrompt = ref('')

const generating = ref(false)
const generatedImage = computed(() => workspace.currentImage)

onMounted(async () => {
  loadingMeta.value = true
  try {
    const [ps, ts] = await Promise.all([getProviders(), getTemplates()])
    providers.value = ps
    templates.value = ts
    const configured = ps.find((p) => p.configured)
    selectedProvider.value = (configured || ps[0])?.name || ''
  } catch {
    ElMessage.error('加载配置失败')
  } finally {
    loadingMeta.value = false
  }
})

const currentProvider = computed(
  () => providers.value.find((p) => p.name === selectedProvider.value) || null
)

const sizeOptions = computed(() => {
  if (!currentProvider.value) return ['1024x1024']
  const sizes = currentProvider.value.supportedSizes
  if (!sizes || !sizes.length) return ['1024x1024']
  if (!sizes.includes(selectedSize.value)) selectedSize.value = sizes[0]
  return sizes
})

const keyConfigured = computed(() => currentProvider.value?.configured ?? false)

async function handleGenerate() {
  if (!concept.value.trim()) { ElMessage.warning('请输入图标概念'); return }
  if (!selectedProvider.value) { ElMessage.warning('请选择 AI 服务商'); return }
  if (!keyConfigured.value) {
    ElMessage.warning(`${currentProvider.value?.displayName} 未配置，请在设置页配置 API Key`)
    return
  }
  generating.value = true
  workspace.clear()
  try {
    const params = {
      concept: concept.value,
      style: selectedStyle.value,
      size: selectedSize.value,
      provider: selectedProvider.value,
      extra: [bgColor.value ? `背景：${bgColor.value}` : '', details.value, extraPrompt.value].filter(Boolean).join('，') || undefined,
    }
    console.log('[生成] 请求:', JSON.stringify(params, null, 2))
    const result = await generateIcon(params)
    workspace.setImage(result.image, result.icon_id)
    ElMessage.success('生成成功')
  } catch (e: any) {
    console.error('[生成] 失败:', e)
    const detail = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    ElMessage.error(`生成失败：${detail}`)
  } finally {
    generating.value = false
  }
}

function goEdit() {
  if (!workspace.currentImage) return
  router.push('/edit')
}
</script>

<template>
  <div class="gen-root" v-loading="loadingMeta">
    <h2 class="page-title">生成图标</h2>

    <div class="gen-body">
      <!-- 画布：预览 -->
      <div class="canvas-area" v-loading="generating" element-loading-text="AI 正在创作...">
        <div class="canvas-inner checkerboard" v-if="generatedImage">
          <img :src="toDataUrl(generatedImage)" class="preview-img" alt="生成的图标" />
          <el-button type="primary" @click="goEdit" style="margin-top:16px">去编辑导出 →</el-button>
        </div>
        <el-empty v-else description="图标将显示在这里" :image-size="120" />
      </div>

      <!-- 右侧工具栏 -->
      <div class="side-panel">
        <el-card>
          <el-form label-position="top" size="default">
            <!-- Step 1: 主体 -->
            <div class="step"><span class="step-num">1</span> 图标主题</div>
            <el-form-item>
              <el-input v-model="concept" placeholder="例如：咖啡杯、数字090、火箭..." maxlength="200" />
            </el-form-item>

            <!-- Step 2: 风格 -->
            <div class="step"><span class="step-num">2</span> 选择风格</div>
            <el-form-item>
              <el-select v-model="selectedStyle" style="width:100%" :key="templates.length">
                <el-option v-for="t in templates" :key="t.id" :label="t.name" :value="t.id">
                  <span class="opt-name">{{ t.name }}</span>
                  <span class="opt-desc">{{ t.description }}</span>
                </el-option>
              </el-select>
            </el-form-item>

            <!-- Step 3: 背景 & 细节 -->
            <div class="step"><span class="step-num">3</span> 背景和细节 <span class="label-hint">— 可选</span></div>
            <el-form-item>
              <el-input v-model="bgColor" placeholder="背景：橙色底、蓝色渐变、透明..." maxlength="100" style="margin-bottom:8px" />
              <el-input v-model="details" placeholder="细节：白色文字、粗体、圆角..." maxlength="200" />
            </el-form-item>

            <!-- Step 4: 补充 -->
            <div class="step"><span class="step-num">4</span> 补充指令 <span class="label-hint">— 可选</span></div>
            <el-form-item>
              <el-input v-model="extraPrompt" type="textarea" :rows="2" placeholder="还想补充什么？例如：参考 iOS 18 风格..." maxlength="300" />
            </el-form-item>

            <el-divider content-position="left">设置</el-divider>

            <el-form-item label="AI 服务商">
              <el-select v-model="selectedProvider" style="width:100%" :key="providers.length">
                <el-option v-for="p in providers" :key="p.name" :label="p.displayName" :value="p.name">
                  <span>{{ p.displayName }}</span>
                  <el-tag v-if="p.configured" size="small" type="success" class="key-tag">已配置</el-tag>
                </el-option>
              </el-select>
            </el-form-item>

            <el-form-item label="尺寸">
              <el-select v-model="selectedSize" style="width:100%">
                <el-option v-for="s in sizeOptions" :key="s" :label="s" :value="s" />
              </el-select>
            </el-form-item>

            <el-button type="primary" :loading="generating" @click="handleGenerate" size="large" style="width:100%">
              {{ generating ? '生成中...' : '生成图标' }}
            </el-button>

          </el-form>
        </el-card>
      </div>
    </div>
  </div>
</template>

<style scoped>
.gen-root { display: flex; flex-direction: column; height: calc(100vh - 110px); }

.page-title { margin: 0 0 12px; font-size: 18px; flex-shrink: 0; }

.gen-body { flex: 1; display: flex; gap: 16px; min-height: 0; }

/* 画布 */
.canvas-area {
  flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
  border-radius: 6px; min-width: 0;
}

.canvas-inner {
  width: 100%; min-height: 300px; display: flex; flex-direction: column; align-items: center; justify-content: center;
  border-radius: 6px; padding: 24px; flex: 1;
}

.checkerboard {
  background-image:
    linear-gradient(45deg, var(--el-border-color-lighter) 25%, transparent 25%),
    linear-gradient(-45deg, var(--el-border-color-lighter) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, var(--el-border-color-lighter) 75%),
    linear-gradient(-45deg, transparent 75%, var(--el-border-color-lighter) 75%);
  background-size: 20px 20px; background-position: 0 0, 0 10px, 10px -10px, -10px 0px;
  background-color: var(--el-fill-color-lighter);
}

.preview-img { max-width: 100%; max-height: 450px; object-fit: contain; }

.canvas-actions { margin-top: 12px; }

/* 工具栏 */
.side-panel { width: 280px; flex-shrink: 0; overflow-y: auto; }

.opt-name { float: left; }
.opt-desc { float: right; color: var(--el-text-color-secondary); font-size: 12px; }
.key-tag { margin-left: 8px; }

.label-hint { font-weight: normal; font-size: 12px; color: var(--el-text-color-secondary); }

.step { font-weight: 600; font-size: 14px; margin-bottom: 6px; }
.step-num { display: inline-block; width: 20px; height: 20px; line-height: 20px; text-align: center; background: var(--el-color-primary); color: #fff; border-radius: 50%; font-size: 12px; margin-right: 4px; }

.quick-words { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
.quick-tag { cursor: pointer; }
.quick-tag:hover { filter: brightness(0.9); }
</style>
