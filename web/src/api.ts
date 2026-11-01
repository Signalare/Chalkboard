// SPDX-License-Identifier: MIT OR Apache-2.0

import type { Category, CecStatus, Item, NodeStatus, Page, Screen, Template } from './types';

const API_BASE = '/api/v1';

async function request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
  const url = `${API_BASE}${endpoint}`;
  const headers = {
    'Content-Type': 'application/json',
    ...(options.headers || {}),
  };
  const response = await fetch(url, { ...options, headers });
  if (!response.ok) {
    const errorText = await response.text();
    throw new Error(`API error ${response.status}: ${errorText}`);
  }
  return response.json();
}

export const api = {
  getStatus(): Promise<NodeStatus> {
    return request<NodeStatus>('/status');
  },

  getCategories(): Promise<Category[]> {
    return request<Category[]>('/categories');
  },

  saveCategory(category: Category): Promise<Category> {
    return request<Category>('/categories', {
      method: 'POST',
      body: JSON.stringify(category),
    });
  },

  deleteCategory(id: string): Promise<{ status: string }> {
    return request<{ status: string }>(`/categories/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    });
  },

  getItems(): Promise<Item[]> {
    return request<Item[]>('/items');
  },

  saveItem(item: Item): Promise<Item> {
    return request<Item>('/items', {
      method: 'POST',
      body: JSON.stringify(item),
    });
  },

  deleteItem(id: string): Promise<{ status: string }> {
    return request<{ status: string }>(`/items/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    });
  },

  setItemSoldOut(id: string, soldOut: boolean): Promise<{ status: string }> {
    return request<{ status: string }>(`/items/${encodeURIComponent(id)}/sold-out`, {
      method: 'POST',
      body: JSON.stringify({ sold_out: soldOut }),
    });
  },

  getTemplates(): Promise<Template[]> {
    return request<Template[]>('/templates');
  },

  getPages(): Promise<Page[]> {
    return request<Page[]>('/pages');
  },

  savePage(page: Page): Promise<Page> {
    return request<Page>('/pages', {
      method: 'POST',
      body: JSON.stringify(page),
    });
  },

  deletePage(id: string): Promise<{ status: string }> {
    return request<{ status: string }>(`/pages/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    });
  },

  getScreens(): Promise<Screen[]> {
    return request<Screen[]>('/screens');
  },

  saveScreen(screen: Screen): Promise<Screen> {
    return request<Screen>('/screens', {
      method: 'POST',
      body: JSON.stringify(screen),
    });
  },

  publish(): Promise<{ status: string; hash: string }> {
    return request<{ status: string; hash: string }>('/publish', {
      method: 'POST',
    });
  },

  getCecStatus(): Promise<CecStatus> {
    return request<CecStatus>('/cec/status');
  },

  sendCecPower(
    action: 'on' | 'standby' | 'active-source',
  ): Promise<{ status: string; action: string; power_status?: string }> {
    return request<{ status: string; action: string; power_status?: string }>('/cec/power', {
      method: 'POST',
      body: JSON.stringify({ action }),
    });
  },
};
