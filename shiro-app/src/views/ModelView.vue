<script setup lang="ts">
// Model 视图：两个分区——
// 注册分区「模型导入」：供应商 + 模型（可检索下拉）+ API Key，保存即注册/更新档案。
// 管理分区「模型管理」：已注册档案列表（供应商 · 当前模型 · 密钥状态），
// 可设默认供应商 / 编辑 / 删除。协议与端点是预设内部实现，不暴露。
import { computed, onMounted } from 'vue'
import { PROVIDERS, modelStore, profileLabel } from '../stores/model'
import Icon from '../components/Icon.vue'
import SearchSelect from '../components/SearchSelect.vue'

onMounted(() => {
  if (!modelStore.loaded) void modelStore.load()
})

/** 渲染用的供应商清单（预设 + 已存自定义配置的注入项） */
const visibleProviders = computed(() => {
  const list = PROVIDERS
  const extras = modelStore.profiles.filter((p) => !PROVIDERS.some((x) => x.key === p.key))
  return [...list, ...extras.map((p) => ({
    key: p.key,
    label: p.key,
    baseUrl: p.base_url,
    models: [p.model],
    protocol: p.protocol as 'openai' | 'anthropic',
  }))]
})

const providerLabels = computed(() => visibleProviders.value.map((p) => p.label))

/** 供应商下拉：label 显示，选中后按 key 切换 */
const providerText = computed({
  get: () => visibleProviders.value.find((p) => p.key === modelStore.provider)?.label ?? '',
  set: (v: string) => {
    const hit = visibleProviders.value.find((p) => p.label === v)
    if (hit && hit.key !== modelStore.provider) modelStore.selectProvider(hit.key)
  },
})

/** 当前编辑中的档案是否已注册（同 key 已存在 → 更新语义） */
const editingRegistered = computed(() =>
  modelStore.profiles.some((p) => p.key === modelStore.provider),
)

/** 可保存：已选供应商 + 模型就位；新建需 Key，更新可留空保留 */
const canSave = computed(() => {
  if (!modelStore.provider || !modelStore.model.trim()) return false
  const registered = editingRegistered.value
  const hasKey = registered
    ? true // 已注册档案 Key 已存
    : modelStore.profiles.find((p) => p.key === modelStore.provider)?.api_key_set ||
      !!modelStore.apiKey.trim()
  return hasKey
})

const keyPlaceholder = computed(() => {
  const existing = modelStore.profiles.find((p) => p.key === modelStore.provider)
  if (existing?.api_key_set) {
    return `已保存（${existing.api_key_preview}），留空则保留`
  }
  return '粘贴 API Key'
})

/** 思考强度选项（'' 不启用） */
const THINKING_OPTIONS = [
  { value: 'low', label: '低' },
  { value: 'medium', label: '中' },
  { value: 'high', label: '高' },
]

const sortedProfiles = computed(() => {
  // 默认档案排在最前
  const list = [...modelStore.profiles]
  list.sort((a, b) => {
    if (a.key === modelStore.defaultKey) return -1
    if (b.key === modelStore.defaultKey) return 1
    return 0
  })
  return list
})
</script>

<template>
  <section class="model-view">
    <!-- ============ 注册分区：模型导入 ============ -->
    <template v-if="modelStore.view === 'import'">
      <header class="head">
        <h2>模型导入</h2>
        <p class="hint">
          选择供应商与模型，填入 API Key 即可注册。已注册的供应商可在「模型管理」中查看与切换默认。
        </p>
      </header>

      <p v-if="modelStore.error" class="hint error">{{ modelStore.error }}</p>

      <div class="form">
        <!-- 供应商 + 模型：一行两列同宽下拉 -->
        <div class="row2">
          <div class="field">
            <span class="field-label">供应商</span>
            <SearchSelect v-model="providerText" :options="providerLabels" strict placeholder="选择供应商" />
          </div>
          <div class="field">
            <span class="field-label">模型</span>
            <SearchSelect
              v-model="modelStore.model"
              :options="modelStore.modelOptions"
              placeholder="输入关键词检索，或手填模型名"
            />
          </div>
        </div>

        <!-- API Key（与上方下拉同宽） -->
        <div class="field">
          <span class="field-label">API Key</span>
          <div class="key-wrap">
            <input
              v-model="modelStore.apiKey"
              :type="modelStore.showKey ? 'text' : 'password'"
              :placeholder="keyPlaceholder"
              spellcheck="false"
              autocomplete="off"
            />
            <button
              class="eye"
              :title="modelStore.showKey ? '隐藏' : '显示'"
              @click="modelStore.showKey = !modelStore.showKey"
            >
              <Icon name="eye" :size="14" />
            </button>
          </div>
        </div>

        <!-- 思考强度（可选；仅对支持推理参数的模型生效） -->
        <div class="field">
          <span class="field-label">
            思考强度（可选）
            <span class="dim">· 仅对支持推理参数的模型生效，启用后建议先「测试」验证</span>
          </span>
          <div class="thinking-pills">
            <button
              v-for="opt in THINKING_OPTIONS"
              :key="opt.value"
              class="pill"
              :class="{ on: modelStore.thinking === opt.value }"
              @click="modelStore.thinking = modelStore.thinking === opt.value ? '' : opt.value"
            >
              {{ opt.label }}
            </button>
          </div>
        </div>

        <div class="actions">
          <span v-if="modelStore.savedOk" class="saved-tip">已保存 ✓</span>
          <button
            class="btn primary"
            :disabled="!canSave || modelStore.saving"
            @click="modelStore.save()"
          >
            {{ modelStore.saving ? '保存中…' : editingRegistered ? '保存修改' : '注册' }}
          </button>
        </div>
      </div>
    </template>

    <!-- ============ 管理分区：模型管理 ============ -->
    <template v-else>
      <header class="head">
        <h2>模型管理</h2>
        <p class="hint">
          已注册的供应商与其当前模型。「默认供应商」是拆解等任务实际使用的档案。
        </p>
      </header>

      <p v-if="modelStore.error" class="hint error">{{ modelStore.error }}</p>
      <p v-if="!modelStore.profiles.length" class="hint empty">
        还没有已注册的供应商。点左侧「模型导入」注册一个。
      </p>

      <div v-else class="profiles">
        <div
          v-for="p in sortedProfiles"
          :key="p.key"
          class="profile"
          :class="{ default: p.key === modelStore.defaultKey }"
        >
          <div class="info">
            <div class="line1">
              <span class="name">{{ profileLabel(p) }}</span>
              <span v-if="p.key === modelStore.defaultKey" class="badge default">默认供应商</span>
            </div>
            <div class="line2">
              <span class="model">{{ p.model }}</span>
              <span v-if="p.thinking" class="think-badge">思考·{{ p.thinking }}</span>
              <span class="dim">·</span>
              <span class="dim">{{ p.api_key_set ? `Key ${p.api_key_preview}` : '未配置 Key' }}</span>
            </div>
          </div>
          <div class="ops">
            <button
              class="btn slim"
              :disabled="modelStore.testing[p.key] === 'run'"
              @click="modelStore.testProfile(p.key)"
            >
              {{ modelStore.testing[p.key] === 'run' ? '测试中…' : '测试' }}
            </button>
            <button class="icon-btn" title="编辑（模型 / Key）" @click="modelStore.editProfile(p)">
              <Icon name="edit" :size="14" />
            </button>
            <button class="icon-btn danger" title="删除档案" @click="modelStore.remove(p.key)">
              <Icon name="trash" :size="14" />
            </button>
          </div>
          <p
            v-if="modelStore.testResults[p.key]"
            class="test-result"
            :class="{ ok: modelStore.testResults[p.key]?.ok, err: !modelStore.testResults[p.key]?.ok }"
          >
            {{ modelStore.testResults[p.key]?.message }}
          </p>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.model-view {
  padding: 4px 8px;
  max-width: 640px;
}

.head h2 {
  margin: 0 0 6px;
  font-size: calc(16px * var(--font-scale-ui));
}

.head .hint {
  margin: 0 0 16px;
  line-height: 1.7;
}

.form {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
}

.row2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

@media (max-width: 480px) {
  .row2 {
    grid-template-columns: 1fr;
  }
}

.key-wrap {
  position: relative;
}

.key-wrap input {
  width: 100%;
  height: 32px;
  padding: 0 32px 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
  background: var(--bg);
  color: var(--text);
  box-sizing: border-box;
}

.key-wrap input:focus {
  border-color: var(--accent);
}

.eye {
  position: absolute;
  right: 4px;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 4px;
  background: none;
  color: var(--text-dim);
  cursor: pointer;
}

.thinking-pills {
  display: flex;
  gap: 6px;
}

.thinking-pills .pill {
  padding: 4px 14px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--bg);
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  cursor: pointer;
}

.thinking-pills .pill.on {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}

.actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 10px;
}

.saved-tip {
  color: #3a9a50;
  font-size: calc(12px * var(--font-scale-ui));
}

.btn {
  height: 30px;
  padding: 0 20px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
}

.btn.slim {
  flex: none;
  height: 28px;
  padding: 0 12px;
  font-size: calc(12px * var(--font-scale-ui));
}

.btn.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.hint {
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
}

.hint.error {
  color: #d05050;
}

.test-result {
  flex-basis: 100%;
  margin: 0;
  font-size: calc(12px * var(--font-scale-ui));
}

.test-result.ok {
  color: #3a9a50;
}

.test-result.err {
  color: #c05050;
}

/* ---- 管理分区：档案列表 ---- */
.profiles {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.profile {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg);
}

.profile.default {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.line1 {
  display: flex;
  align-items: center;
  gap: 8px;
}

.name {
  font-size: calc(14px * var(--font-scale-ui));
  font-weight: 600;
}

.badge.default {
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--accent);
  color: #fff;
  font-size: calc(11px * var(--font-scale-ui));
}

.line2 {
  display: flex;
  gap: 6px;
  align-items: baseline;
  font-size: calc(12px * var(--font-scale-ui));
}

.model {
  color: var(--text);
}

.think-badge {
  margin-left: 6px;
  padding: 0 6px;
  border-radius: 999px;
  border: 1px solid var(--border);
  color: var(--text-dim);
  font-size: calc(10px * var(--font-scale-ui));
}

.dim {
  color: var(--text-dim);
}

.ops {
  flex: none;
  display: flex;
  align-items: center;
  gap: 4px;
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .icon-btn:hover {
    background: var(--border);
    color: var(--text);
  }

  .icon-btn.danger:hover {
    color: #c05050;
  }
}
</style>
