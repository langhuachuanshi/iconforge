<script setup lang="ts">
import { computed, ref, h, onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ElNotification, ElButton } from 'element-plus'
import { hasDesktopShortcut, createDesktopShortcut, getConfig, setConfig } from './api/client'

const route = useRoute()
const router = useRouter()
const activeIndex = computed(() => route.path)

const appWindow = getCurrentWindow()
const isMaximized = ref(false)

function handleSelect(index: string) {
  router.push(index)
}

appWindow.onResized(async () => {
  isMaximized.value = await appWindow.isMaximized()
})

// 桌面快捷方式提示：首次运行或桌面无快捷方式时弹通知（右下角）
// "创建快捷方式" → 创建后关闭；"不再提示" → 持久化配置后关闭
onMounted(async () => {
  try {
    const cfg = await getConfig()
    if (cfg['shortcut_prompt_dismissed'] === '1') return
    if (await hasDesktopShortcut()) return

    const notifyInstance = ElNotification({
      title: '创建桌面快捷方式',
      position: 'bottom-right',
      duration: 0, // 不自动关闭，等用户操作
      showClose: true,
      // 用 h() 渲染带两个按钮的消息体（ElNotification 原生不支持 actions）
      message: h('div', { style: 'display:flex;gap:8px;margin-top:6px' }, [
        h(ElButton, {
          size: 'small',
          type: 'primary',
          onClick: async () => {
            const ok = await createDesktopShortcut()
            notifyInstance.close()
            if (ok) {
              ElNotification({ title: '已创建', message: '桌面快捷方式已添加', position: 'bottom-right', duration: 2500 })
            }
          },
        }, () => '创建快捷方式'),
        h(ElButton, {
          size: 'small',
          text: true,
          onClick: async () => {
            await setConfig('shortcut_prompt_dismissed', '1')
            notifyInstance.close()
          },
        }, () => '不再提示'),
      ]),
    })
  } catch {
    // 静默失败，不打扰用户
  }
})
</script>

<template>
  <div class="app-layout">
    <!-- 标题栏 -->
    <header class="titlebar" data-tauri-drag-region>
      <div class="titlebar-brand">
        <img src="/icon.png" class="titlebar-icon" alt="" />
        <span class="titlebar-title">IconForge</span>
      </div>
      <div class="titlebar-controls">
        <el-button text class="win-btn" @click="appWindow.minimize()">
          <el-icon><Minus /></el-icon>
        </el-button>
        <el-button text class="win-btn" @click="appWindow.toggleMaximize()">
          <el-icon><FullScreen v-if="!isMaximized" /><CopyDocument v-else /></el-icon>
        </el-button>
        <el-button text class="win-btn win-btn--close" @click="appWindow.close()">
          <el-icon><Close /></el-icon>
        </el-button>
      </div>
    </header>

    <!-- Header + Aside + Main -->
    <el-container class="app-body">
      <el-aside width="220px">
        <el-menu
          :default-active="activeIndex"
          @select="handleSelect"
        >
          <el-menu-item index="/generate">
            <el-icon><MagicStick /></el-icon>
            <span>生成图标</span>
          </el-menu-item>
          <el-menu-item index="/edit">
            <el-icon><Crop /></el-icon>
            <span>编辑图标</span>
          </el-menu-item>
          <el-menu-item index="/export">
            <el-icon><Download /></el-icon>
            <span>导出图标</span>
          </el-menu-item>
          <el-menu-item index="/extract">
            <el-icon><CopyDocument /></el-icon>
            <span>图标提取</span>
          </el-menu-item>
          <el-menu-item index="/history">
            <el-icon><Clock /></el-icon>
            <span>历史记录</span>
          </el-menu-item>
          <el-menu-item index="/settings">
            <el-icon><Setting /></el-icon>
            <span>设置</span>
          </el-menu-item>
        </el-menu>
      </el-aside>

      <el-main>
        <router-view v-slot="{ Component }">
          <keep-alive>
            <component :is="Component" />
          </keep-alive>
        </router-view>
      </el-main>
    </el-container>
  </div>
</template>

<style>
.app-layout {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}

/* 标题栏 */
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 48px;
  padding-left: 12px;
  -webkit-app-region: drag;
  user-select: none;
}

.titlebar-brand {
  display: flex; align-items: center; gap: 6px;
}

.titlebar-icon { width: 32px; height: 32px; }

.titlebar-title { font-size: 13px; font-weight: 600; }

.titlebar-controls {
  display: flex;
  -webkit-app-region: no-drag;
}

.win-btn {
  width: 48px;
  height: 48px;
  border-radius: 0;
  font-size: 14px;
}

.win-btn--close:hover {
  background-color: var(--el-color-danger);
  color: var(--el-color-white);
}

/* 主体 */
.app-body {
  flex: 1;
}

.app-body .el-aside {
  padding-bottom: 20px;
}

.app-body .el-menu {
  height: 100%;
  border-right-width: 1px;
}

.app-body .el-menu-item {
  height: 44px;
  line-height: 44px;
}
</style>
