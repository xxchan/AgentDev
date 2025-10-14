const rawBase = process.env.NEXT_PUBLIC_AGENTDEV_API_BASE;

const normalizedBase =
  typeof rawBase === 'string' && rawBase.length > 0
    ? rawBase.replace(/\/+$/, '')
    : '';

export function apiUrl(path: string) {
  if (!normalizedBase || /^https?:\/\//i.test(path)) {
    return path;
  }

  if (!path.startsWith('/')) {
    return `${normalizedBase}/${path}`;
  }

  return `${normalizedBase}${path}`;
}

