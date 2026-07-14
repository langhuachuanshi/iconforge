<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { useRouter } from 'vue-router'
import {
  deleteIcon,
  fetchIconBase64,
  listIcons,
  toDataUrl,
  type IconMeta,
} from '../api/client'
import { useWorkspaceStore } from '../stores/workspace'

const router = useRouter()
const workspace = useWorkspaceStore()

const icons = ref<IconMeta[]>([])
const loading = ref(false)
// iconId → data URL 映射（用于缩略图）
const thumbUrls = ref<Record<string, string>>({})

onMounted(async () => {
  await loadIcons()
})

async function loadIcons() {
  loading.value = true
  try {
    icons.value = await listIcons()
    for (const icon of icons.value) {
      try {
        const result = await fetchIconBase64(icon.id)
        thumbUrls.value[icon.id] = toDataUrl(result)
      } catch {
        thumbUrls.value[icon.id] = ''
      }
    }
  } catch {
    ElMessage.error('加载历史记录失败')
  } finally {
    loading.value = false
  }
}

/** 载入到工作区并跳转编辑页 */
async function handleReuse(icon: IconMeta) {
  try {
    const base64 = await fetchIconBase64(icon.id)
    workspace.setImage(base64, icon.id)
    ElMessage.success('已载入，跳转编辑页')
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
    ElMessage.success('已删除')
  } catch {
    ElMessage.error('删除失败')
  }
}
</script>

<template>
  <div v-loading="loading">
    <div class="header-row">
      <h2 class="page-title">历史记录</h2>
      <el-button text @click="loadIcons" :loading="loading">
        <el-icon><Refresh /></el-icon> 刷新
      </el-button>
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
        :body-style="{ padding: '0' }"
        shadow="hover"
      >
        <div class="icon-thumb">
          <img
            v-if="thumbUrls[icon.id]"
            :src="thumbUrls[icon.id]"
            :alt="icon.concept"
            loading="lazy"
          />
          <el-icon v-else :size="32"><Picture /></el-icon>
        </div>
        <div class="icon-info">
          <div class="info-concept" :title="icon.concept">
            {{ icon.concept || '(未命名)' }}
          </div>
          <div class="info-actions">
            <el-button size="small" type="primary" @click="handleReuse(icon)">
              载入编辑
            </el-button>
            <el-button size="small" type="danger" plain @click="handleDelete(icon)">
              删除
            </el-button>
          </div>
        </div>
      </el-card>
    </div>
  </div>
</template>

<style scoped>
.header-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.page-title {
  margin: 0;
  font-size: 22px;
}

.icon-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 16px;
}

.icon-card {
  overflow: hidden;
}

.icon-thumb {
  width: 100%;
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--el-fill-color-lighter);
  background-image:
    linear-gradient(45deg, var(--el-border-color-lighter) 25%, transparent 25%),
    linear-gradient(-45deg, var(--el-border-color-lighter) 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, var(--el-border-color-lighter) 75%),
    linear-gradient(-45deg, transparent 75%, var(--el-border-color-lighter) 75%);
  background-size: 20px 20px;
  background-position: 0 0, 0 10px, 10px -10px, -10px 0px;
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

.info-actions {
  display: flex;
  gap: 8px;
}
</style>
