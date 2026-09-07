// 内容库（Database）视图状态：侧栏分类选择 + 全局人物库角色列表/详情/编辑（API 见 shiro-daemon/src/assets.rs）
import { computed, reactive } from 'vue'
import { apiFetch } from '../api'
import type { components } from '../api-types'

export type CharacterSummary = components['schemas']['CharacterSummary']
export type CharacterDetail = components['schemas']['CharacterDetail']

/** 侧栏分类：characters 角色库 / craft 角色制作（拆解任务） */
export type DbCategory = 'characters' | 'craft'

export const dbStore = reactive({
  /** 当前选中的分类（默认角色） */
  category: 'characters' as DbCategory,
  /** 角色列表 */
  characters: [] as CharacterSummary[],
  /** 列表是否拉取过（切换面板不重复拉，手动刷新强制） */
  charactersLoaded: false,
  loading: false,
  error: '',
  /** 选中的角色 id */
  selectedId: null as string | null,
  detail: null as CharacterDetail | null,
  detailLoading: false,
  /** 列表筛选：文本（匹配名字/原型/标签/角色/来源） */
  filter: '',
  /** 列表筛选：功能角色（null = 全部） */
  roleFilter: null as string | null,
  /** 保存后 frontmatter 解析失败的提示（编辑器顶部横幅；空 = 无问题） */
  parseError: '',

  selectCategory(key: DbCategory) {
    if (this.category === key) return
    this.category = key
    this.selectedId = null
    this.detail = null
    if (key === 'characters') void this.refreshCharacters()
  },

  /** keepStale：刷新后选中项已不在列表时仍保留选中（编辑中保存了暂不合法的内容，别把编辑器抽走） */
  async refreshCharacters(keepStale = false) {
    this.loading = true
    this.error = ''
    try {
      const res = await apiFetch<components['schemas']['CharacterListResponse']>(
        '/api/v1/db/characters',
      )
      this.characters = res.characters ?? []
      this.charactersLoaded = true
      if (
        !keepStale &&
        this.selectedId &&
        !this.characters.some((c) => c.id === this.selectedId)
      ) {
        this.selectedId = null
        this.detail = null
      }
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.loading = false
    }
  },

  selectCharacter(id: string) {
    if (this.selectedId === id) return
    this.selectedId = id
    this.parseError = ''
    void this.loadCharacter(id)
  },

  async loadCharacter(id: string) {
    this.detailLoading = true
    try {
      this.detail = await apiFetch<CharacterDetail>(
        `/api/v1/db/characters/${encodeURIComponent(id)}`,
      )
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.detailLoading = false
    }
  },

  /** 保存角色卡（完整原始内容）。返回是否解析通过；解析失败时内容已保存但会从列表消失 */
  async saveCharacter(id: string, content: string): Promise<boolean> {
    this.error = ''
    try {
      const res = await apiFetch<components['schemas']['SaveCharacterResponse']>(
        `/api/v1/db/characters/${encodeURIComponent(id)}`,
        {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ content }),
        },
      )
      this.parseError = res.parsed ? '' : (res.parse_error ?? '解析失败')
      if (!res.parsed) return false
      await this.refreshCharacters(true)
      await this.loadCharacter(id)
      return true
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
      return false
    }
  },

  async createCharacter(name: string) {
    this.error = ''
    try {
      const res = await apiFetch<CharacterDetail>('/api/v1/db/characters', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      })
      await this.refreshCharacters(true)
      this.selectedId = res.id
      await this.loadCharacter(res.id)
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },
})

/** 应用筛选后的角色列表（客户端过滤，列表已在内存） */
export const filteredCharacters = computed(() => {
  const q = dbStore.filter.trim().toLowerCase()
  return dbStore.characters.filter((c) => {
    if (dbStore.roleFilter && c.role !== dbStore.roleFilter) return false
    if (!q) return true
    const hay = [c.name, c.role ?? '', ...c.archetype, ...c.tags, c.source ?? '']
      .join(' ')
      .toLowerCase()
    return hay.includes(q)
  })
})

/** 列表中出现过的功能角色（筛选胶囊用） */
export const characterRoles = computed(() => {
  const roles = new Set<string>()
  for (const c of dbStore.characters) if (c.role) roles.add(c.role)
  return [...roles]
})
