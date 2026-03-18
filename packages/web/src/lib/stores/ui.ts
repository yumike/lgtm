import { writable } from 'svelte/store';

export const selectedFile = writable<string | null>(null);
export const error = writable<string | null>(null);
