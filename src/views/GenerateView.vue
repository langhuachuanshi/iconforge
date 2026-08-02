<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
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

// ── 基础输入 ──
const concept = ref('')
const selectedStyle = ref('flat-design')

// ── 背景可视化 ──
type BgMode = 'auto' | 'transparent' | 'solid' | 'gradient' | 'custom'
const bgMode = ref<BgMode>('auto')
const solidColor = ref('#ffffff')
const gradientColor = ref('') // 选中的渐变预设
const GRADIENTS = [
  ['#FF6B6B', '#FFD93D'],
  ['#6BCB77', '#4D96FF'],
  ['#A8E6CF', '#DCEDC1'],
  ['#FF9A9E', '#FECFEF'],
  ['#4158D0', '#C850C0'],
  ['#0093E9', '#80D0C7'],
]
const bgCustomText = ref('')

// ── 细节标签（多选）──
const detailTags = ref<string[]>([])
const DETAIL_OPTIONS = [
  { label: '简约', en: 'minimalist' },
  { label: '精致', en: 'detailed, refined' },
  { label: '科技感', en: 'futuristic, tech' },
  { label: '可爱', en: 'cute, kawaii' },
  { label: '立体', en: '3D look' },
  { label: '扁平', en: 'flat' },
  { label: '高对比', en: 'high contrast' },
  { label: '柔和', en: 'soft, gentle' },
  { label: '粗线条', en: 'bold outlines' },
  { label: '渐变', en: 'gradient' },
]

function toggleDetail(label: string) {
  const i = detailTags.value.indexOf(label)
  if (i >= 0) detailTags.value.splice(i, 1)
  else detailTags.value.push(label)
}

// 模板按 category 分组（el-option-group）
const groupedTemplates = computed(() => {
  const order: string[] = []
  const map = new Map<string, Template[]>()
  for (const t of templates.value) {
    if (!map.has(t.category)) { map.set(t.category, []); order.push(t.category) }
    map.get(t.category)!.push(t)
  }
  return order.map((label) => ({ label, items: map.get(label)! }))
})

// 折叠面板 v-model（数组）
const previewOpenNames = ref<string[]>([])
const advancedNames = ref<string[]>(['adv'])

// ── 补充指令（进阶自由文本）──
const extraPrompt = ref('')

// ── 高级设置 ──
const selectedProvider = ref('')
const selectedSize = ref('1024x1024')
const negativePrompt = ref('')
const seedInput = ref<string>('') // 字符串方便留空判断，提交时转数字
const genCount = ref(1)

const generating = ref(false)

// 多图结果：[{ b64, iconId }]
const results = ref<{ b64: string; iconId: string }[]>([])

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

// 切换服务商时重置尺寸到该服务商第一档
watch(selectedProvider, () => {
  const sizes = currentProvider.value?.supportedSizes
  if (sizes && sizes.length && !sizes.includes(selectedSize.value)) {
    selectedSize.value = sizes[0]
  }
})

// ── 提示词预览：与后端 generate_icon 拼接规则保持一致 ──
const currentTemplate = computed(
  () => templates.value.find((t) => t.id === selectedStyle.value) || null
)

const extraSegment = computed(() => {
  const parts: string[] = []
  // 背景
  if (bgMode.value === 'transparent') parts.push('transparent background')
  else if (bgMode.value === 'solid' && solidColor.value) parts.push(`solid ${solidColor.value} background`)
  else if (bgMode.value === 'gradient' && gradientColor.value) parts.push(`${gradientColor.value} gradient background`)
  else if (bgMode.value === 'custom' && bgCustomText.value.trim()) parts.push(bgCustomText.value.trim())
  // 细节标签 → 英文
  if (detailTags.value.length) {
    const ens = detailTags.value.map((label) => DETAIL_OPTIONS.find((o) => o.label === label)?.en).filter(Boolean)
    if (ens.length) parts.push(ens.join(', '))
  }
  // 补充
  if (extraPrompt.value.trim()) parts.push(extraPrompt.value.trim())
  return parts.join('. ')
})

const finalPrompt = computed(() => {
  const prefix = currentTemplate.value?.promptPrefix?.replace('{concept}', concept.value || '...') ?? ''
  let p = prefix
  if (extraSegment.value) { p += '. '; p += extraSegment.value }
  p += '. Centered composition, professional app icon, readable at small sizes'
  return p
})

// ── 生成 ──
async function handleGenerate() {
  if (!concept.value.trim()) { ElMessage.warning('请输入图标概念'); return }
  if (!selectedProvider.value) { ElMessage.warning('请选择 AI 服务商'); return }
  if (!keyConfigured.value) {
    ElMessage.warning(`${currentProvider.value?.displayName} 未配置，请在设置页配置 API Key`)
    return
  }
  generating.value = true
  results.value = []
  workspace.clear()

  const baseParams = {
    concept: concept.value,
    style: selectedStyle.value,
    size: selectedSize.value,
    provider: selectedProvider.value,
    extra: extraSegment.value || undefined,
    negativePrompt: negativePrompt.value.trim() || undefined,
    seed: seedInput.value.trim() ? Number(seedInput.value.trim()) : null,
  }

  // 并发 N 次（通义万相 n 锁死为 1，前端循环对三家都兼容）
  const n = Math.min(Math.max(genCount.value, 1), 4)
  const tasks = Array.from({ length: n }, () => generateIcon(baseParams).then(
    (r) => ({ ok: true as const, b64: r.image, iconId: r.icon_id }),
    (e) => ({ ok: false as const, e }),
  ))
  console.log('[生成] 请求:', JSON.stringify({ ...baseParams, n }, null, 2))

  try {
    const settled = await Promise.all(tasks)
    const ok = settled.filter((s): s is { ok: true; b64: string; iconId: string } => s.ok)
    const fail = settled.length - ok.length
    if (ok.length === 0) {
      const first = settled[0]
      const detail = first && !first.ok ? (typeof first.e === 'string' ? first.e : (first.e?.message || JSON.stringify(first.e))) : '未知错误'
      throw new Error(detail)
    }
    results.value = ok
    // 第一张作为 workspace 主图（去编辑用）
    workspace.setImage(ok[0].b64, ok[0].iconId)
    ElMessage.success(`生成 ${ok.length} 张${fail > 0 ? `（${fail} 张失败）` : ''}`)
  } catch (e: any) {
    console.error('[生成] 失败:', e)
    const detail = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    ElMessage.error(`生成失败：${detail}`)
  } finally {
    generating.value = false
  }
}

function useResult(idx: number) {
  const r = results.value[idx]
  if (!r) return
  workspace.setImage(r.b64, r.iconId)
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
      <!-- 画布：结果预览 -->
      <div class="canvas-area" v-loading="generating" element-loading-text="AI 正在创作...">
        <!-- 多图网格 -->
        <div v-if="results.length" class="result-grid" :class="{ single: results.length === 1 }">
          <div
            v-for="(r, idx) in results"
            :key="idx"
            class="result-cell checkerboard"
            @click="useResult(idx)"
          >
            <img :src="toDataUrl(r.b64)" class="result-img" alt="生成结果" />
          </div>
        </div>
        <el-empty v-else description="图标将显示在这里" :image-size="120" />

        <div v-if="results.length" class="canvas-actions">
          <el-button text size="small">点击图片选中</el-button>
          <el-button type="primary" size="small" @click="goEdit">去编辑 →</el-button>
        </div>
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

            <!-- Step 2: 风格（按分类） -->
            <div class="step"><span class="step-num">2</span> 选择风格</div>
            <el-form-item>
              <el-select v-model="selectedStyle" style="width:100%" :key="templates.length">
                <el-option-group v-for="grp in groupedTemplates" :key="grp.label" :label="grp.label">
                  <el-option v-for="t in grp.items" :key="t.id" :label="t.name" :value="t.id">
                    <span class="opt-name">{{ t.name }}</span>
                    <span class="opt-desc">{{ t.description }}</span>
                  </el-option>
                </el-option-group>
              </el-select>
            </el-form-item>

            <!-- Step 3: 背景 -->
            <div class="step"><span class="step-num">3</span> 背景 <span class="label-hint">— 可选</span></div>
            <el-form-item>
              <div class="bg-modes">
                <el-radio-group v-model="bgMode" size="small">
                  <el-radio-button value="auto">自动</el-radio-button>
                  <el-radio-button value="transparent">透明</el-radio-button>
                  <el-radio-button value="solid">纯色</el-radio-button>
                  <el-radio-button value="gradient">渐变</el-radio-button>
                  <el-radio-button value="custom">自定义</el-radio-button>
                </el-radio-group>
              </div>
              <el-color-picker v-if="bgMode === 'solid'" v-model="solidColor" size="small" style="margin-top:8px" />
              <div v-if="bgMode === 'gradient'" class="gradient-grid">
                <button
                  v-for="(g, i) in GRADIENTS"
                  :key="i"
                  class="gradient-swatch"
                  :class="{ active: gradientColor === `${g[0]} to ${g[1]}` }"
                  :style="{ background: `linear-gradient(135deg, ${g[0]}, ${g[1]})` }"
                  :title="`${g[0]} → ${g[1]}`"
                  @click="gradientColor = `${g[0]} to ${g[1]}`"
                />
              </div>
              <el-input
                v-if="bgMode === 'custom'"
                v-model="bgCustomText"
                placeholder="例如：深蓝星空背景"
                maxlength="100"
                style="margin-top:8px"
              />
            </el-form-item>

            <!-- Step 4: 细节标签 -->
            <div class="step"><span class="step-num">4</span> 细节 <span class="label-hint">— 可选</span></div>
            <el-form-item>
              <div class="tag-chips">
                <button
                  v-for="opt in DETAIL_OPTIONS"
                  :key="opt.label"
                  class="tag-chip"
                  :class="{ active: detailTags.includes(opt.label) }"
                  @click="toggleDetail(opt.label)"
                >
                  {{ opt.label }}
                </button>
              </div>
            </el-form-item>

            <!-- Step 5: 补充指令 -->
            <div class="step"><span class="step-num">5</span> 补充指令 <span class="label-hint">— 可选</span></div>
            <el-form-item>
              <el-input v-model="extraPrompt" type="textarea" :rows="2" placeholder="还想补充什么？例如：参考 iOS 18 风格..." maxlength="300" />
            </el-form-item>

            <!-- 提示词预览（折叠） -->
            <el-collapse v-model="previewOpenNames">
              <el-collapse-item title="查看完整提示词" name="prompt">
                <pre class="prompt-preview">{{ finalPrompt }}</pre>
                <p v-if="negativePrompt.trim()" class="neg-preview">负向：{{ negativePrompt.trim() }}</p>
              </el-collapse-item>
            </el-collapse>

            <!-- 高级设置（折叠） -->
            <el-collapse v-model="advancedNames">
              <el-collapse-item title="高级设置" name="adv">
                <el-form-item label="AI 服务商">
                  <el-select v-model="selectedProvider" style="width:100%" :key="providers.length">
                    <el-option v-for="p in providers" :key="p.name" :label="p.displayName" :value="p.name">
                      <span>{{ p.displayName }}</span>
                      <el-tag v-if="p.configured" size="small" type="success" class="key-tag">已配置</el-tag>
                    </el-option>
                  </el-select>
                </el-form-item>
                <el-form-item label="生成数量">
                  <el-radio-group v-model="genCount" size="small">
                    <el-radio-button :value="1">1 张</el-radio-button>
                    <el-radio-button :value="2">2 张</el-radio-button>
                    <el-radio-button :value="3">3 张</el-radio-button>
                    <el-radio-button :value="4">4 张</el-radio-button>
                  </el-radio-group>
                </el-form-item>
                <el-form-item label="尺寸">
                  <el-select v-model="selectedSize" style="width:100%">
                    <el-option v-for="s in sizeOptions" :key="s" :label="s" :value="s" />
                  </el-select>
                </el-form-item>
                <el-form-item label="负向提示词">
                  <el-input v-model="negativePrompt" type="textarea" :rows="2" placeholder="不希望出现的内容，如：文字、模糊、变形" maxlength="300" />
                </el-form-item>
                <el-form-item label="随机种子">
                  <el-input v-model="seedInput" placeholder="留空=随机；填数字可复现" />
                </el-form-item>
              </el-collapse-item>
            </el-collapse>

            <el-button type="primary" :loading="generating" @click="handleGenerate" size="large" style="width:100%; margin-top:8px">
              {{ generating ? '生成中...' : `生成${genCount > 1 ? ` ${genCount} 张` : ''}` }}
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

/* 多图网格 */
.result-grid {
  display: grid; gap: 12px; padding: 16px;
  width: 100%; flex: 1; align-content: center; justify-content: center;
}
.result-grid:not(.single) { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.result-grid.single { display: flex; justify-content: center; }

.result-cell {
  border-radius: 8px; overflow: hidden; cursor: pointer;
  border: 2px solid transparent; transition: border-color 0.15s;
  max-width: 100%; aspect-ratio: 1 / 1; display: flex; align-items: center; justify-content: center;
}
.result-cell:hover { border-color: var(--el-color-primary); }
.result-img { max-width: 100%; max-height: 100%; object-fit: contain; }

.canvas-actions { display: flex; align-items: center; gap: 12px; margin-top: 12px; }

/* 工具栏 */
.side-panel { width: 320px; flex-shrink: 0; overflow-y: auto; }

.opt-name { float: left; }
.opt-desc { float: right; color: var(--el-text-color-secondary); font-size: 12px; }
.key-tag { margin-left: 8px; }

.label-hint { font-weight: normal; font-size: 12px; color: var(--el-text-color-secondary); }

.step { font-weight: 600; font-size: 14px; margin-bottom: 6px; }
.step-num { display: inline-block; width: 20px; height: 20px; line-height: 20px; text-align: center; background: var(--el-color-primary); color: #fff; border-radius: 50%; font-size: 12px; margin-right: 4px; }

/* 背景模式 */
.bg-modes { display: flex; flex-wrap: wrap; gap: 4px; }
.gradient-grid { display: grid; grid-template-columns: repeat(6, 1fr); gap: 6px; margin-top: 8px; }
.gradient-swatch {
  width: 100%; aspect-ratio: 1 / 1; border-radius: 6px; cursor: pointer;
  border: 2px solid transparent; transition: transform 0.1s, border-color 0.15s;
}
.gradient-swatch:hover { transform: scale(1.08); }
.gradient-swatch.active { border-color: var(--el-color-primary); box-shadow: 0 0 0 2px var(--el-color-primary-light-7); }

/* 细节标签 chips */
.tag-chips { display: flex; flex-wrap: wrap; gap: 6px; }
.tag-chip {
  padding: 4px 10px; border-radius: 14px; font-size: 12px; cursor: pointer;
  background: var(--el-fill-color-light); color: var(--el-text-color-regular);
  border: 1px solid transparent; transition: all 0.15s;
}
.tag-chip:hover { background: var(--el-fill-color); }
.tag-chip.active {
  background: var(--el-color-primary-light-9); color: var(--el-color-primary);
  border-color: var(--el-color-primary);
}

/* 提示词预览 */
.prompt-preview {
  margin: 0; padding: 10px; background: var(--el-fill-color-light); border-radius: 4px;
  font-size: 12px; line-height: 1.6; color: var(--el-text-color-regular);
  white-space: pre-wrap; word-break: break-word; font-family: ui-monospace, monospace;
}
.neg-preview { margin: 8px 0 0; font-size: 12px; color: var(--el-text-color-secondary); }
</style>
