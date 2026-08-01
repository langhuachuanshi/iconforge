import { createRouter, createWebHashHistory } from 'vue-router'

// Tauri 使用 file:// 协议，不支持 HTML5 history 模式，必须用 hash 路由
const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: '/',
      redirect: '/generate',
    },
    {
      path: '/generate',
      name: 'generate',
      component: () => import('../views/GenerateView.vue'),
      meta: { title: '生成图标' },
    },
    {
      path: '/edit',
      name: 'edit',
      component: () => import('../views/EditView.vue'),
      meta: { title: '编辑图标' },
    },
    {
      path: '/export',
      name: 'export',
      component: () => import('../views/ExportView.vue'),
      meta: { title: '导出图标' },
    },
    {
      path: '/history',
      name: 'history',
      component: () => import('../views/HistoryView.vue'),
      meta: { title: '历史记录' },
    },
    {
      path: '/extract',
      name: 'extract',
      component: () => import('../views/ExtractView.vue'),
      meta: { title: '图标提取' },
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('../views/SettingsView.vue'),
      meta: { title: '设置' },
    },
  ],
})

export default router
