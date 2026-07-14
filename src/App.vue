<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'

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
            <span>编辑导出</span>
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
        <router-view />
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

.titlebar-icon { width: 18px; height: 18px; }

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
