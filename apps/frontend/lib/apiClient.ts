'use client';

import { apiUrl } from '@/lib/api';

interface ApiErrorInit {
  status: number;
  statusText: string;
  body?: unknown;
}

export class ApiError extends Error {
  readonly status: number;
  readonly statusText: string;
  readonly body?: unknown;

  constructor(message: string, { status, statusText, body }: ApiErrorInit) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.statusText = statusText;
    this.body = body;
  }
}

async function parseBody(response: Response): Promise<unknown> {
  const contentType = response.headers.get('content-type');
  if (contentType && contentType.toLowerCase().includes('application/json')) {
    return response.json();
  }
  const text = await response.text();
  return text.length ? text : null;
}

async function assertOk(response: Response): Promise<void> {
  if (response.ok) {
    return;
  }

  const body = await parseBody(response);
  const fallback = `Request failed with status ${response.status}`;
  let message = fallback;

  if (body && typeof body === 'object') {
    const data = body as Record<string, unknown>;
    const reason = typeof data.message === 'string' ? data.message : null;
    message = reason ?? fallback;
  } else if (typeof body === 'string' && body.trim().length > 0) {
    message = body.trim();
  }

  throw new ApiError(message, {
    status: response.status,
    statusText: response.statusText,
    body,
  });
}

type JsonHeaders = HeadersInit | undefined;

function mergeHeaders(defaults: HeadersInit, override?: JsonHeaders): HeadersInit {
  if (!override) {
    return defaults;
  }

  const result = new Headers(defaults);
  const entries = override instanceof Headers ? override.entries() : Object.entries(override);
  for (const [key, value] of entries) {
    if (typeof value === 'undefined') {
      continue;
    }
    result.set(key, Array.isArray(value) ? value.join(', ') : value);
  }
  return result;
}

interface RequestOptions extends Omit<RequestInit, 'method' | 'body'> {
  signal?: AbortSignal;
}

export async function getJson<TResponse>(
  path: string,
  options: RequestOptions = {},
): Promise<TResponse> {
  const { headers, ...rest } = options;
  const response = await fetch(apiUrl(path), {
    method: 'GET',
    headers: mergeHeaders(
      {
        Accept: 'application/json',
      },
      headers,
    ),
    ...rest,
  });

  await assertOk(response);
  return response.json() as Promise<TResponse>;
}

export interface PostJsonOptions extends RequestOptions {
  headers?: HeadersInit;
}

export async function postJson<TResponse, TBody>(
  path: string,
  body: TBody,
  options: PostJsonOptions = {},
): Promise<TResponse> {
  const { headers, ...rest } = options;
  const response = await fetch(apiUrl(path), {
    method: 'POST',
    body: body != null ? JSON.stringify(body) : undefined,
    headers: mergeHeaders(
      {
        Accept: 'application/json',
        'Content-Type': 'application/json',
      },
      headers,
    ),
    ...rest,
  });

  await assertOk(response);
  if (response.status === 204) {
    return {} as TResponse;
  }
  return response.json() as Promise<TResponse>;
}
