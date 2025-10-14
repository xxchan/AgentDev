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

export function websocketUrl(path: string) {
  if (/^wss?:\/\//i.test(path)) {
    return path;
  }

  if (!normalizedBase) {
    return path;
  }

  const wsBase = normalizedBase.replace(/^http/i, 'ws');

  if (!path.startsWith('/')) {
    return `${wsBase}/${path}`;
  }

  return `${wsBase}${path}`;
}
