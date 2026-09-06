import { createApp } from 'vue'
import App from './App.vue'
import './style.css'
// 霞鹜文楷（内置默认字体，OFL-1.1 自托管；400 常规 + 700 粗体，unicode-range 分包按需加载）
import '@fontsource/lxgw-wenkai'
import '@fontsource/lxgw-wenkai/700.css'
import {
  applyContentWidth,
  applyEditorFont,
  applyEditorFontSize,
  applyEditorLineHeight,
  applyEditorParaGap,
  applyPreviewLineHeight,
  applyPreviewParaGap,
  applyUiFont,
  applyUiFontSize,
  currentContentWidth,
  currentEditorFontKey,
  currentEditorFontSize,
  currentEditorLineHeight,
  currentEditorParaGap,
  currentPreviewLineHeight,
  currentPreviewParaGap,
  currentUiFontKey,
  currentUiFontSize,
} from './utils/font'
import { vOverlayScrollbar } from './utils/overlayScrollbar'

applyUiFont(currentUiFontKey())
applyEditorFont(currentEditorFontKey())
applyUiFontSize(currentUiFontSize())
applyEditorFontSize(currentEditorFontSize())
applyEditorLineHeight(currentEditorLineHeight())
applyEditorParaGap(currentEditorParaGap())
applyContentWidth(currentContentWidth())
applyPreviewLineHeight(currentPreviewLineHeight())
applyPreviewParaGap(currentPreviewParaGap())

createApp(App).directive('overlay-scrollbar', vOverlayScrollbar).mount('#app')
