const PROVIDER_BADGE_CLASS_MAP: Record<string, string> = {
  codex: "bg-zinc-100 text-zinc-700",
  kimi: "bg-blue-50 text-blue-700",
  // Normalize all Claude variants to the same accent
  claude: "bg-orange-50 text-orange-700",
};

const DEFAULT_PROVIDER_BADGE_CLASSES = "bg-slate-100 text-slate-700";

function normalizeProviderKey(provider: string): string {
  return provider.trim().toLowerCase();
}

export function getProviderBadgeClasses(provider: string): string {
  const normalized = normalizeProviderKey(provider);
  if (PROVIDER_BADGE_CLASS_MAP[normalized]) {
    return PROVIDER_BADGE_CLASS_MAP[normalized];
  }

  if (normalized.startsWith("codex")) {
    return PROVIDER_BADGE_CLASS_MAP.codex;
  }

  if (normalized.startsWith("kimi")) {
    return PROVIDER_BADGE_CLASS_MAP.kimi;
  }

  if (normalized.startsWith("claude")) {
    return PROVIDER_BADGE_CLASS_MAP.claude;
  }

  return DEFAULT_PROVIDER_BADGE_CLASSES;
}
