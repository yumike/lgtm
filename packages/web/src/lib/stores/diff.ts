import { writable } from 'svelte/store';
import type { DiffFile } from '../types';

export const diffFiles = writable<DiffFile[]>([]);
