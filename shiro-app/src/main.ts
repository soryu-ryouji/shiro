import { createApp } from 'vue'
import App from './App.vue'
import './style.css'
// 霞鹜文楷（内置默认字体，OFL-1.1 自托管；400 常规 + 700 粗体，unicode-range 分包按需加载）
import '@fontsource/lxgw-wenkai'
import '@fontsource/lxgw-wenkai/700.css'
import { applyEditorFont, applyEditorFontSize, applyUiFont, applyUiFontSize, currentEditorFontKey, currentEditorFontSize, currentUiFontKey, currentUiFontSize } from './utils/font'

applyUiFont(currentUiFontKey())
applyEditorFont(currentEditorFontKey())
applyUiFontSize(currentUiFontSize())
applyEditorFontSize(currentEditorFontSize())

createApp(App).mount('#app')
