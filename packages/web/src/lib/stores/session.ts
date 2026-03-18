import { writable } from 'svelte/store';
import type { Session } from '../types';

export const session = writable<Session | null>(null);
