import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'msvc-kit',
  description: 'A portable MSVC Build Tools installer and manager',
  base: '/msvc-kit/',

  locales: {
    root: {
      label: 'English',
      lang: 'en',
    },
    zh: {
      label: '中文',
      lang: 'zh-CN',
      link: '/zh/',
      themeConfig: {
        // Pages that only exist in English are linked with an "（英文）"
        // marker so the zh navigation never points at a missing file.
        nav: [
          { text: '指南', link: '/zh/guide/getting-started' },
          { text: 'API', link: '/zh/api/library' },
          { text: '示例（英文）', link: '/examples/basic' },
          {
            text: 'DCC 集成（英文）',
            items: [
              { text: 'Unreal Engine 5', link: '/dcc/unreal-engine' },
              { text: 'Maya', link: '/dcc/maya' },
              { text: 'Houdini', link: '/dcc/houdini' },
              { text: '3ds Max', link: '/dcc/3dsmax' },
            ],
          },
        ],
        sidebar: {
          '/zh/guide/': [
            {
              text: '介绍',
              items: [
                { text: '什么是 msvc-kit?（英文）', link: '/guide/what-is-msvc-kit' },
                { text: '快速开始', link: '/zh/guide/getting-started' },
                { text: '安装', link: '/zh/guide/installation' },
              ],
            },
            {
              text: 'CLI 使用',
              items: [
                { text: '下载', link: '/zh/guide/cli-download' },
                { text: '设置环境（英文）', link: '/guide/cli-setup' },
                { text: '配置', link: '/zh/guide/cli-config' },
                { text: '列表和清理（英文）', link: '/guide/cli-list-clean' },
                { text: '查询', link: '/zh/guide/cli-query' },
              ],
            },
            {
              text: '高级',
              items: [
                { text: '缓存机制（英文）', link: '/guide/caching' },
                { text: '架构支持', link: '/zh/guide/architecture' },
                { text: 'Visual Studio 版本', link: '/zh/guide/vs-versions' },
                { text: 'GitHub Action', link: '/zh/guide/github-action' },
                { text: 'CI/CD 集成（英文）', link: '/guide/ci-cd' },
                { text: '退出码行为（英文）', link: '/exit-code-behavior' },
              ],
            },
          ],
          '/zh/api/': [
            {
              text: '库 API',
              items: [
                { text: '概述', link: '/zh/api/library' },
                { text: 'DownloadOptions', link: '/zh/api/download-options' },
                { text: 'InstallInfo', link: '/zh/api/install-info' },
                { text: 'MsvcEnvironment', link: '/zh/api/msvc-environment' },
                { text: 'ToolPaths', link: '/zh/api/tool-paths' },
                { text: 'QueryResult', link: '/zh/api/query-result' },
              ],
            },
          ],
        },
      },
    },
  },

  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'API', link: '/api/library' },
      { text: 'Examples', link: '/examples/basic' },
      {
        text: 'DCC Integration',
        items: [
          { text: 'Unreal Engine 5', link: '/dcc/unreal-engine' },
          { text: 'Maya', link: '/dcc/maya' },
          { text: 'Houdini', link: '/dcc/houdini' },
          { text: '3ds Max', link: '/dcc/3dsmax' },
        ],
      },
    ],

    sidebar: {
      '/guide/': [
        {
          text: 'Introduction',
          items: [
            { text: 'What is msvc-kit?', link: '/guide/what-is-msvc-kit' },
            { text: 'Getting Started', link: '/guide/getting-started' },
            { text: 'Installation', link: '/guide/installation' },
          ],
        },
        {
          text: 'CLI Usage',
          items: [
            { text: 'Download', link: '/guide/cli-download' },
            { text: 'Setup Environment', link: '/guide/cli-setup' },
            { text: 'Configuration', link: '/guide/cli-config' },
            { text: 'List & Clean', link: '/guide/cli-list-clean' },
            { text: 'Query', link: '/guide/cli-query' },
          ],
        },
        {
          text: 'Advanced',
          items: [
            { text: 'Caching Mechanism', link: '/guide/caching' },
            { text: 'Architecture Support', link: '/guide/architecture' },
            { text: 'Visual Studio Versions', link: '/guide/vs-versions' },
            { text: 'GitHub Action', link: '/guide/github-action' },
            { text: 'CI/CD Integration', link: '/guide/ci-cd' },
            { text: 'Exit Code Behavior', link: '/exit-code-behavior' },
          ],
        },
      ],
      '/api/': [
        {
          text: 'Library API',
          items: [
            { text: 'Overview', link: '/api/library' },
            { text: 'DownloadOptions', link: '/api/download-options' },
            { text: 'InstallInfo', link: '/api/install-info' },
            { text: 'MsvcEnvironment', link: '/api/msvc-environment' },
            { text: 'ToolPaths', link: '/api/tool-paths' },
            { text: 'QueryResult', link: '/api/query-result' },
          ],
        },
      ],
      '/examples/': [
        {
          text: 'Examples',
          items: [
            { text: 'Basic Usage', link: '/examples/basic' },
            { text: 'Custom Paths', link: '/examples/custom-paths' },
            { text: 'Build Script', link: '/examples/build-script' },
            { text: 'Quick Compile', link: '/examples/quick-compile' },
          ],
        },
      ],
      '/dcc/': [
        {
          text: 'DCC Integration',
          items: [
            { text: 'Overview', link: '/dcc/overview' },
            { text: 'Unreal Engine 5', link: '/dcc/unreal-engine' },
            { text: 'Maya', link: '/dcc/maya' },
            { text: 'Houdini', link: '/dcc/houdini' },
            { text: '3ds Max', link: '/dcc/3dsmax' },
            { text: 'Blender', link: '/dcc/blender' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/loonghao/msvc-kit' },
    ],

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2024-present loonghao',
    },

    search: {
      provider: 'local',
    },
  },
})
