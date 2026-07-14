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

// 表单状态
const concept = ref('')
const selectedStyle = ref('flat-design')
const selectedProvider = ref('')
const selectedSize = ref('1024x1024')
const useCustomPrompt = ref(false)
const customPrompt = ref('')

const generating = ref(false)

// 从 store 读当前图片（切换路由回来仍在）
const generatedImage = computed(() => workspace.currentImage)

onMounted(async () => {
  loadingMeta.value = true
  try {
    const [ps, ts] = await Promise.all([getProviders(), getTemplates()])
    providers.value = ps
    templates.value = ts
    // 默认选第一个已配置 Key 的 Provider，否则选第一个
    const configured = ps.find((p) => p.configured)
    selectedProvider.value = (configured || ps[0])?.name || ''
  } catch {
    ElMessage.error('加载配置失败，请确认后端已启动')
  } finally {
    loadingMeta.value = false
  }
})

const currentProvider = computed(
  () => providers.value.find((p) => p.name === selectedProvider.value) || null
)

// 当前 Provider 支持的尺寸
const sizeOptions = computed(() => {
  if (!currentProvider.value) return ['1024x1024']
  const sizes = currentProvider.value.supportedSizes
  if (!sizes || sizes.length === 0) return ['1024x1024']
  if (!sizes.includes(selectedSize.value)) {
    selectedSize.value = sizes[0]
  }
  return sizes
})

// 当前 Provider 是否已在后端配置 Key
const keyConfigured = computed(() => currentProvider.value?.configured ?? false)

async function handleGenerate() {
  if (!concept.value.trim()) {
    ElMessage.warning('请输入图标概念')
    return
  }
  if (!selectedProvider.value) {
    ElMessage.warning('请选择 AI 服务商')
    return
  }
  if (!keyConfigured.value) {
    ElMessage.warning(
      `${currentProvider.value?.displayName} 未配置，请在设置页配置 API Key`
    )
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
      custom_prompt: useCustomPrompt.value ? customPrompt.value : undefined,
    }
    console.log('[生成] 请求参数:', JSON.stringify(params, null, 2))
    const result = await generateIcon(params)
    console.log('[生成] 成功:', result.icon_id)
    workspace.setImage(result.image, result.icon_id)
    ElMessage.success('生成成功，已自动保存到历史记录')
  } catch (e: any) {
    console.error('[生成] 失败:', e)
    const detail = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    ElMessage.error(`生成失败：${detail}`)
  } finally {
    generating.value = false
  }
}

function handleSendToEdit() {
  if (!workspace.currentImage) return
  router.push('/edit')
}
</script>

<template>
  <div v-loading="loadingMeta">
    <h2 class="page-title">生成图标</h2>

    <el-row :gutter="24">
      <!-- 左：配置表单 -->
      <el-col :xs="24" :md="12">
        <el-card>
          <el-form label-position="top">
            <el-form-item label="图标概念">
              <el-input
                v-model="concept"
                placeholder="描述你要的图标，如：咖啡杯、火箭、邮件"
                maxlength="500"
                show-word-limit
                @keyup.enter="handleGenerate"
              />
            </el-form-item>

            <el-divider content-position="left">风格</el-divider>

            <el-form-item v-if="!useCustomPrompt" label="选择风格模板">
              <el-select
                v-model="selectedStyle"
                placeholder="选择风格"
                style="width: 100%"
              >
                <el-option
                  v-for="t in templates"
                  :key="t.id"
                  :label="t.name"
                  :value="t.id"
                >
                  <span class="option-name">{{ t.name }}</span>
                  <span class="option-desc">{{ t.description }}</span>
                </el-option>
              </el-select>
            </el-form-item>

            <el-form-item>
              <el-checkbox v-model="useCustomPrompt">
                自定义提示词（覆盖模板）
              </el-checkbox>
            </el-form-item>

            <el-form-item v-if="useCustomPrompt" label="自定义提示词">
              <el-input
                v-model="customPrompt"
                type="textarea"
                :rows="3"
                placeholder="可用 {concept} 占位符引用上面的概念，如 'a minimalist icon of {concept}'"
              />
            </el-form-item>

            <el-divider content-position="left">生成设置</el-divider>

            <el-form-item label="AI 服务商">
              <el-select
                v-model="selectedProvider"
                placeholder="选择服务商"
                style="width: 100%"
                :key="providers.length"
              >
                <el-option
                  v-for="p in providers"
                  :key="p.name"
                  :label="p.displayName"
                  :value="p.name"
                >
                  <span>{{ p.displayName }}</span>
                  <el-tag
                    v-if="p.configured"
                    size="small"
                    type="success"
                    class="key-tag"
                  >
                    已配置
                  </el-tag>
                </el-option>
              </el-select>
            </el-form-item>

            <el-form-item label="尺寸">
              <el-select v-model="selectedSize" style="width: 100%">
                <el-option
                  v-for="s in sizeOptions"
                  :key="s"
                  :label="s"
                  :value="s"
                />
              </el-select>
            </el-form-item>

            <el-form-item>
              <el-button
                type="primary"
                :loading="generating"
                @click="handleGenerate"
                size="large"
                class="generate-btn"
              >
                {{ generating ? '生成中...' : '生成图标' }}
              </el-button>
            </el-form-item>
          </el-form>
        </el-card>
      </el-col>

      <!-- 右：预览 -->
      <el-col :xs="24" :md="12">
        <el-card>
          <template #header>
            <div class="preview-header">
              <span>预览</span>
              <el-button
                v-if="generatedImage"
                type="primary"
                text
                @click="handleSendToEdit"
              >
                去编辑导出 →
              </el-button>
            </div>
          </template>
          <div class="preview-area" v-loading="generating" element-loading-text="AI 正在创作中...">
            <el-empty v-if="!generatedImage && !generating" description="生成的图标会显示在这里" />
            <img
              v-if="generatedImage"
              :src="toDataUrl(generatedImage)"
              class="preview-image"
              alt="生成的图标"
            />
          </div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<style scoped>
.page-title {
  margin: 0 0 16px;
  font-size: 22px;
}

.option-name {
  float: left;
}

.option-desc {
  float: right;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.key-tag {
  margin-left: 8px;
}

.generate-btn {
  width: 100%;
}

.preview-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.preview-area {
  min-height: 400px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--el-fill-color-lighter);
  border-radius: 4px;
  padding: 24px;
}

.preview-image {
  max-width: 100%;
  max-height: 500px;
  object-fit: contain;
}
</style>
