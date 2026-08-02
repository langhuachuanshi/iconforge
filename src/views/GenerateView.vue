<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { useRoute, useRouter } from 'vue-router'
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
const route = useRoute()
const workspace = useWorkspaceStore()

const providers = ref<ProviderInfo[]>([])
const templates = ref<Template[]>([])
const loadingMeta = ref(false)

// ── 基础输入 ──
const concept = ref('')
const selectedStyle = ref('flat-design')
// 提示词模式：guide 引导式（点选）/ expert 专家式（直接输入完整 prompt）
type PromptMode = 'guide' | 'expert'
const promptMode = ref<PromptMode>('guide')
// 专家模式：用户直接输入完整提示词（不经过模板/背景/细节拼接）
const expertPrompt = ref('')

// ── 背景可视化 ──
type BgMode = 'auto' | 'transparent' | 'solid' | 'gradient' | 'custom'
const bgMode = ref<BgMode>('auto')
const solidColor = ref('#ffffff')
const gradientColor = ref('') // 选中的渐变预设（"c1 to c2"）
const gradientCustomA = ref('#4158D0') // 自定义渐变起始色
const gradientCustomB = ref('#C850C0') // 自定义渐变结束色
const GRADIENTS = [
  ['#FF6B6B', '#FFD93D'],
  ['#6BCB77', '#4D96FF'],
  ['#A8E6CF', '#DCEDC1'],
  ['#FF9A9E', '#FECFEF'],
  ['#4158D0', '#C850C0'],
  ['#0093E9', '#80D0C7'],
]
const bgCustomText = ref('')

// 渐变最终取色：自定义色非空时优先，否则用预设
const gradientFinal = computed(() => {
  if (gradientCustomA.value && gradientCustomB.value) {
    return `${gradientCustomA.value} to ${gradientCustomB.value}`
  }
  return gradientColor.value
})

// ── 细节标签（多选）+ 自定义描述 ──
const detailTags = ref<string[]>([])
const detailCustom = ref('')
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
const advancedNames = ref<string[]>([])

// ── 补充指令（进阶自由文本）──
const extraPrompt = ref('')

// ── 高级设置 ──
const selectedProvider = ref('')
const selectedSize = ref('1024x1024')
const negativePrompt = ref('')
const seedInput = ref<string>('') // 字符串方便留空判断，提交时转数字
const genCount = ref(1)

const generating = ref(false)
const genTotal = ref(0)  // 本轮生成总数
const genCurrent = ref(0) // 当前正在生成第几张
// 等待计时：直观显示已等多久，方便判断是否卡死
const elapsedSec = ref(0)
let elapsedTimer: ReturnType<typeof setInterval> | null = null
function startElapsed() {
  stopElapsed()
  elapsedSec.value = 0
  elapsedTimer = setInterval(() => { elapsedSec.value += 1 }, 1000)
}
function stopElapsed() {
  if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null }
  elapsedSec.value = 0
}
onUnmounted(stopElapsed)

// 多图结果：[{ b64, iconId }]
const results = ref<{ b64: string; iconId: string }[]>([])
// 当前选中的结果索引（去编辑用这张），生成后默认 0
const selectedIdx = ref(0)

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

  // 批量重生成跳转：读取 query 填充（部分重现：concept+style，其余 concept 列表放入补充指令）
  const q = route.query
  if (q.concept) concept.value = String(q.concept)
  if (q.style) {
    const exists = templates.value.some((t) => t.id === q.style)
    if (exists) selectedStyle.value = String(q.style)
  }
  if (q.concepts) {
    const list = String(q.concepts).split('\n').filter(Boolean)
    const extra = list.filter((c) => c !== q.concept)
    if (extra.length > 0) {
      extraPrompt.value = `同批待生成：${extra.join('、')}`
    }
    if (list.length > 1) {
      ElMessage.info(`已载入第 1 张参数，共 ${list.length} 张待重生成`)
    }
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

// 生成进度文案
const genLoadingText = computed(() => {
  const t = elapsedSec.value > 0 ? `（已等待 ${elapsedSec.value}s）` : ''
  return genTotal.value > 1 ? `AI 正在创作第 ${genCurrent.value}/${genTotal.value} 张...${t}` : `AI 正在创作...${t}`
})
const genBtnText = computed(() => {
  const t = elapsedSec.value > 0 ? ` ${elapsedSec.value}s` : ''
  return genTotal.value > 1 ? `生成中 ${genCurrent.value}/${genTotal.value}...${t}` : `生成中...${t}`
})

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
  else if (bgMode.value === 'gradient' && gradientFinal.value) parts.push(`${gradientFinal.value} gradient background`)
  else if (bgMode.value === 'custom' && bgCustomText.value.trim()) parts.push(bgCustomText.value.trim())
  // 细节标签 → 英文
  if (detailTags.value.length) {
    const ens = detailTags.value.map((label) => DETAIL_OPTIONS.find((o) => o.label === label)?.en).filter(Boolean)
    if (ens.length) parts.push(ens.join(', '))
  }
  // 自定义细节描述
  if (detailCustom.value.trim()) parts.push(detailCustom.value.trim())
  // 补充
  if (extraPrompt.value.trim()) parts.push(extraPrompt.value.trim())
  return parts.join('. ')
})

const finalPrompt = computed(() => {
  // 专家模式：用户输入即最终 prompt（仅追加统一收尾），不走模板/背景/细节拼接
  if (promptMode.value === 'expert') {
    const raw = expertPrompt.value.trim() || '...'
    return `${raw}. Centered composition, professional app icon, readable at small sizes`
  }
  // 引导式：模板前缀 + 背景/细节/补充 + 收尾
  const prefix = currentTemplate.value?.promptPrefix?.replace('{concept}', concept.value || '...') ?? ''
  let p = prefix
  if (extraSegment.value) { p += '. '; p += extraSegment.value }
  p += '. Centered composition, professional app icon, readable at small sizes'
  return p
})

// ── 生成 ──
async function handleGenerate() {
  // 校验：专家模式查 expertPrompt，引导式查 concept
  if (promptMode.value === 'expert') {
    if (!expertPrompt.value.trim()) { ElMessage.warning('请输入提示词'); return }
  } else {
    if (!concept.value.trim()) { ElMessage.warning('请输入图标概念'); return }
  }
  if (!selectedProvider.value) { ElMessage.warning('请选择 AI 服务商'); return }
  if (!keyConfigured.value) {
    ElMessage.warning(`${currentProvider.value?.displayName} 未配置，请在设置页配置 API Key`)
    return
  }
  generating.value = true
  results.value = []
  selectedIdx.value = 0
  workspace.clear()
  startElapsed()

  const isExpert = promptMode.value === 'expert'
  const baseParams = {
    concept: isExpert ? '' : concept.value,
    style: selectedStyle.value,
    size: selectedSize.value,
    provider: selectedProvider.value,
    extra: isExpert ? undefined : (extraSegment.value || undefined),
    negativePrompt: negativePrompt.value.trim() || undefined,
    seed: seedInput.value.trim() ? Number(seedInput.value.trim()) : null,
    rawPrompt: isExpert ? expertPrompt.value.trim() : undefined,
  }

  // 纯串行：一张出完再发下一张，规避服务商 QPS 限流（最稳，不丢图）
  // 单张遇 429 限流自动重试，退避 5 秒，最多 2 次
  const n = Math.min(Math.max(genCount.value, 1), 2)
  console.log('[生成] 请求:', JSON.stringify({ ...baseParams, n }, null, 2))

  /** 带重试的单张生成：429 时退避重试 */
  async function generateWithRetry(params: typeof baseParams, retries = 2): Promise<{ b64: string; iconId: string }> {
    for (let attempt = 0; ; attempt++) {
      try {
        const r = await generateIcon(params)
        return { b64: r.image, iconId: r.icon_id }
      } catch (e) {
        const msg = typeof e === 'string' ? e : (e && typeof e === 'object' && 'message' in e ? String((e as {message: unknown}).message) : '')
        const isRateLimit = /429|rate|limit|throttl/i.test(msg)
        if (attempt < retries && isRateLimit) {
          console.warn(`[生成] 限流，${5}s 后重试 (${attempt + 1}/${retries})`)
          await new Promise((r) => setTimeout(r, 5000))
          continue
        }
        throw e
      }
    }
  }

  let fail = 0
  try {
    for (let i = 0; i < n; i++) {
      genTotal.value = n
      genCurrent.value = i + 1
      try {
        const r = await generateWithRetry(baseParams)
        results.value.push(r)
        // 第一张作为 workspace 主图（去编辑用）
        if (results.value.length === 1) workspace.setImage(r.b64, r.iconId)
      } catch (e) {
        fail++
        console.error(`[生成] 第 ${i + 1} 张失败:`, e)
      }
    }
    if (results.value.length === 0) {
      throw new Error('全部生成失败，请检查网络或服务商配置')
    }
    ElMessage.success(`生成 ${results.value.length} 张${fail > 0 ? `（${fail} 张失败）` : ''}`)
  } catch (e: any) {
    console.error('[生成] 失败:', e)
    const detail = typeof e === 'string' ? e : (e?.message || JSON.stringify(e))
    ElMessage.error(`生成失败：${detail}`)
  } finally {
    generating.value = false
    genTotal.value = 0
    genCurrent.value = 0
    stopElapsed()
  }
}

function useResult(idx: number) {
  const r = results.value[idx]
  if (!r) return
  selectedIdx.value = idx
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
      <div class="canvas-area" v-loading="generating" :element-loading-text="genLoadingText">
        <!-- 多图网格 -->
        <div v-if="results.length" class="result-grid" :class="`cols-${Math.min(results.length, 4)}`">
          <div
            v-for="(r, idx) in results"
            :key="idx"
            class="result-cell checkerboard"
            :class="{ selected: selectedIdx === idx }"
            @click="useResult(idx)"
          >
            <img :src="toDataUrl(r.b64)" class="result-img" alt="生成结果" />
            <span v-if="selectedIdx === idx" class="cell-badge">已选中</span>
          </div>
        </div>
        <el-empty v-else description="图标将显示在这里" :image-size="120" />

        <div v-if="results.length" class="canvas-actions">
          <span class="action-hint">已选中第 {{ selectedIdx + 1 }} 张，点击图片可切换</span>
          <el-button type="primary" size="small" @click="goEdit">去编辑 →</el-button>
        </div>
      </div>

      <!-- 右侧工具栏 -->
      <div class="side-panel">
        <el-card>
          <el-form label-position="top" size="default">
            <!-- 模式切换 -->
            <div class="mode-switch">
              <el-radio-group v-model="promptMode" size="small">
                <el-radio-button value="guide">引导式</el-radio-button>
                <el-radio-button value="expert">专家式</el-radio-button>
              </el-radio-group>
              <span class="mode-hint">{{ promptMode === 'guide' ? '点选配置' : '直接输入完整提示词' }}</span>
            </div>

            <!-- 专家模式：完整提示词输入 -->
            <template v-if="promptMode === 'expert'">
              <el-form-item>
                <el-input
                  v-model="expertPrompt"
                  type="textarea"
                  :rows="8"
                  placeholder="直接输入完整英文提示词，例如：&#10;A 3D rendered app icon of a glowing magic cube, neon edges, dark background, cinematic lighting, no text&#10;&#10;（将原样发送，仅自动追加图标质量收尾）"
                  maxlength="1000"
                  show-word-limit
                />
              </el-form-item>
            </template>

            <!-- 引导式：原有 Step 1-5 -->
            <template v-else>
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
                  type="button"
                  class="gradient-swatch"
                  :class="{ active: gradientColor === `${g[0]} to ${g[1]}` }"
                  :style="{ background: `linear-gradient(135deg, ${g[0]}, ${g[1]})` }"
                  :title="`${g[0]} → ${g[1]}`"
                  @click="gradientColor = `${g[0]} to ${g[1]}`"
                />
              </div>
              <div v-if="bgMode === 'gradient'" class="gradient-custom">
                <span class="tool-desc">或自定义双色：</span>
                <div class="gradient-pickers">
                  <el-color-picker v-model="gradientCustomA" size="small" />
                  <span class="grad-arrow">→</span>
                  <el-color-picker v-model="gradientCustomB" size="small" />
                  <div
                    class="gradient-preview"
                    :style="{ background: `linear-gradient(135deg, ${gradientCustomA}, ${gradientCustomB})` }"
                  />
                </div>
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
                  type="button"
                  class="tag-chip"
                  :class="{ active: detailTags.includes(opt.label) }"
                  @click="toggleDetail(opt.label)"
                >
                  {{ opt.label }}
                </button>
              </div>
              <el-input v-model="detailCustom" placeholder="或输入自定义细节，如：白色文字、数字 090..." maxlength="200" style="margin-top:8px" />
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
            </template><!-- /引导式 -->

            <!-- 高级设置（折叠，两种模式共用） -->
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
              {{ generating ? genBtnText : `生成${genCount > 1 ? ` ${genCount} 张` : ''}` }}
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
  width: 100%; flex: 1; min-height: 0; overflow: auto;
  align-content: center; justify-content: center;
}
/* 按数量定列数：1张1列，2张2列，3-4张2列 */
.result-grid.cols-1 { grid-template-columns: minmax(0, 480px); }
.result-grid.cols-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.result-grid.cols-3 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.result-grid.cols-4 { grid-template-columns: repeat(2, minmax(0, 1fr)); }

.result-cell {
  position: relative; border-radius: 8px; overflow: hidden; cursor: pointer;
  border: 2px solid transparent; transition: border-color 0.15s;
  min-height: 160px; display: flex; align-items: center; justify-content: center;
}
.result-cell:hover { border-color: var(--el-color-primary-light-5); }
.result-cell.selected { border-color: var(--el-color-primary); box-shadow: 0 0 0 2px var(--el-color-primary-light-7); }
.result-img { max-width: 100%; max-height: 320px; object-fit: contain; }
.cell-badge {
  position: absolute; top: 8px; left: 8px;
  padding: 2px 8px; border-radius: 10px; font-size: 11px;
  background: var(--el-color-primary); color: #fff;
}
.action-hint { font-size: 12px; color: var(--el-text-color-secondary); }

.canvas-actions { display: flex; align-items: center; gap: 12px; margin-top: 12px; }

/* 工具栏 */
.side-panel { width: 320px; flex-shrink: 0; overflow-y: auto; }

.opt-name { float: left; }
.opt-desc { float: right; color: var(--el-text-color-secondary); font-size: 12px; }
.key-tag { margin-left: 8px; }

.label-hint { font-weight: normal; font-size: 12px; color: var(--el-text-color-secondary); }

.step { font-weight: 600; font-size: 14px; margin-bottom: 6px; }

/* 模式切换 */
.mode-switch { display: flex; align-items: center; gap: 10px; margin-bottom: 14px; }
.mode-hint { font-size: 12px; color: var(--el-text-color-secondary); }
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

/* 渐变自定义双色 */
.gradient-custom { margin-top: 10px; }
.gradient-pickers { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.grad-arrow { color: var(--el-text-color-secondary); font-size: 14px; }
.gradient-preview {
  width: 40px; height: 20px; border-radius: 4px;
  border: 1px solid var(--el-border-color); margin-left: 4px;
}
.tool-desc { font-size: 12px; color: var(--el-text-color-secondary); }

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
