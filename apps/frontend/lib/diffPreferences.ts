import { DiffModeEnum } from '@git-diff-view/react';

const DIFF_MODE_STORAGE_KEY = 'agentdev.diff.mode';
const DIFF_WRAP_STORAGE_KEY = 'agentdev.diff.wrap';

export function readStoredDiffMode(defaultValue: DiffModeEnum = DiffModeEnum.SplitGitHub): DiffModeEnum {
  if (typeof window === 'undefined') {
    return defaultValue;
  }

  try {
    const raw = window.localStorage.getItem(DIFF_MODE_STORAGE_KEY);
    if (!raw) {
      return defaultValue;
    }

    const parsed = Number.parseInt(raw, 10);
    if (Number.isNaN(parsed)) {
      return defaultValue;
    }

    switch (parsed) {
      case DiffModeEnum.SplitGitHub:
      case DiffModeEnum.SplitGitLab:
      case DiffModeEnum.Split:
      case DiffModeEnum.Unified:
        return parsed as DiffModeEnum;
      default:
        return defaultValue;
    }
  } catch (error) {
    console.warn('Failed to read diff mode preference from localStorage', error);
    return defaultValue;
  }
}

export function writeStoredDiffMode(mode: DiffModeEnum): void {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(DIFF_MODE_STORAGE_KEY, mode.toString());
  } catch (error) {
    console.warn('Failed to persist diff mode preference to localStorage', error);
  }
}

export function readStoredWrapPreference(defaultValue = false): boolean {
  if (typeof window === 'undefined') {
    return defaultValue;
  }

  try {
    const raw = window.localStorage.getItem(DIFF_WRAP_STORAGE_KEY);
    if (raw === null) {
      return defaultValue;
    }
    return raw === 'true';
  } catch (error) {
    console.warn('Failed to read diff wrap preference from localStorage', error);
    return defaultValue;
  }
}

export function writeStoredWrapPreference(value: boolean): void {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(DIFF_WRAP_STORAGE_KEY, value ? 'true' : 'false');
  } catch (error) {
    console.warn('Failed to persist diff wrap preference to localStorage', error);
  }
}
