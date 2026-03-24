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
        nav: [
          { text: '指南', link: '/zh/guide/getting-started' },
          { text: 'API', link: '/zh/api/library' },
          { text: '示例', link: '/zh/examples/basic' },
          {
            text: 'DCC 集成',
            items: [
              { text: 'Unreal Engine 5', link: '/zh/dcc/unreal-engine' },
              { text: 'Maya', link: '/zh/dcc/maya' },
              { text: 'Houdini', link: '/zh/dcc/houdini' },
              { text: '3ds Max', link: '/zh/dcc/3dsmax' },
            ],
          },
        ],
        sidebar: {
          '/zh/guide/': [
            {
              text: '介绍',
              items: [
                { text: '什么是 msvc-kit?', link: '/zh/guide/what-is-msvc-kit' },
                { text: '快速开始', link: '/zh/guide/getting-started' },
                { text: '安装', link: '/zh/guide/installation' },
              ],
            },
            {
              text: 'CLI 使用',
              items: [
                { text: '下载', link: '/zh/guide/cli-download' },
                { text: '设置环境', link: '/zh/guide/cli-setup' },
                { text: '配置', link: '/zh/guide/cli-config' },
                { text: '列表和清理', link: '/zh/guide/cli-list-clean' },
                { text: '查询', link: '/zh/guide/cli-query' },
              ],
            },
            {
              text: '高级',
              items: [
                { text: '缓存机制', link: '/zh/guide/caching' },
                { text: '架构支持', link: '/zh/guide/architecture' },
                { text: 'GitHub Action', link: '/zh/guide/github-action' },
                { text: 'CI/CD 集成', link: '/zh/guide/ci-cd' },
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
          '/zh/examples/': [
            {
              text: '示例',
              items: [
                { text: '基本用法', link: '/zh/examples/basic' },
                { text: '自定义路径', link: '/zh/examples/custom-paths' },
                { text: '构建脚本', link: '/zh/examples/build-script' },
                { text: '快速编译', link: '/zh/examples/quick-compile' },
              ],
            },
          ],
          '/zh/dcc/': [
            {
              text: 'DCC 集成',
              items: [
                { text: '概述', link: '/zh/dcc/overview' },
                { text: 'Unreal Engine 5', link: '/zh/dcc/unreal-engine' },
                { text: 'Maya', link: '/zh/dcc/maya' },
                { text: 'Houdini', link: '/zh/dcc/houdini' },
                { text: '3ds Max', link: '/zh/dcc/3dsmax' },
                { text: 'Blender', link: '/zh/dcc/blender' },
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
            { text: 'GitHub Action', link: '/guide/github-action' },
            { text: 'CI/CD Integration', link: '/guide/ci-cd' },
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
