import type { SettingsSectionKey } from "../../stores/navigationStore";
import type { Settings } from "../../types/settings";

export interface SettingsCatalogEntry {
  id: string;
  section: SettingsSectionKey;
  label: string;
  description: string;
  keywords: string;
  scope: "app" | "project";
  preference?: keyof Settings;
}

export const settingsCatalog: SettingsCatalogEntry[] = [
  { id: "settings-language", section: "general", label: "settings.language.title", description: "settings.language.description", keywords: "语言 中文 english language locale", scope: "app", preference: "language" },
  { id: "settings-theme", section: "appearance", label: "settings.appearance.title", description: "settings.appearance.description", keywords: "主题 明亮 深色 自动 系统 theme dark light auto appearance", scope: "app", preference: "theme" },
  { id: "settings-color-theme", section: "appearance", label: "settings.appearance.colorTheme", description: "settings.appearance.colorDescription", keywords: "配色 color colour workbench parchment graphite verdant nocturne contrast 高对比", scope: "app", preference: "colorThemePreset" },
  { id: "settings-close-behavior", section: "background", label: "settings.background.closeBehavior", description: "settings.background.description", keywords: "关闭 退出 托盘 后台 询问 close quit tray ask background", scope: "app", preference: "closeBehavior" },
  { id: "settings-notifications", section: "background", label: "settings.background.notifications", description: "settings.background.notificationsHelp", keywords: "通知 提醒 权限 notification permission alert", scope: "app", preference: "systemNotifications" },
  { id: "settings-updates", section: "updates", label: "settings.updates.title", description: "settings.updates.description", keywords: "更新 版本 下载 update download version", scope: "app" },
  { id: "settings-ai", section: "ai", label: "settings.ai.title", description: "settings.ai.description", keywords: "ai 模型 密钥 key api provider byok agent cli 默认 路线 context 上下文 tokens openai anthropic ollama", scope: "project", preference: "aiDefaultRoute" },
  { id: "settings-versions", section: "versions", label: "versions.title", description: "settings.catalog.versions", keywords: "版本 历史 恢复 git checkpoint history restore backup", scope: "project" },
  { id: "settings-security", section: "security", label: "settings.security.title", description: "settings.security.description", keywords: "安全 密钥 撤销 凭证 secret credential security revoke", scope: "project" },
  { id: "settings-compatibility", section: "compatibility", label: "settings.nav.compatibility", description: "settings.catalog.compatibility", keywords: "迁移 兼容 旧版 导入 compatibility legacy migration import", scope: "project" },
];

export function preferenceIsModified(entry: SettingsCatalogEntry, settings: Settings, defaults: Settings): boolean {
  return Boolean(entry.preference && JSON.stringify(settings[entry.preference]) !== JSON.stringify(defaults[entry.preference]));
}
