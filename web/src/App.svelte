<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from './api';
  import type { Category, Item, NodeStatus, Page, Screen, Template } from './types';

  type Tab = 'sold-out' | 'menu' | 'templates' | 'screens';

  let currentTab = $state<Tab>('sold-out');
  let status = $state<NodeStatus | null>(null);
  let categories = $state<Category[]>([]);
  let items = $state<Item[]>([]);
  let templates = $state<Template[]>([]);
  let pages = $state<Page[]>([]);
  let screens = $state<Screen[]>([]);
  let loading = $state(true);
  let errorMessage = $state<string | null>(null);
  let feedbackMessage = $state<string | null>(null);
  let filterText = $state('');

  // Selected entities for editing
  let editingCategory = $state<Category | null>(null);
  let editingItem = $state<Item | null>(null);
  let editingPage = $state<Page | null>(null);
  let editingScreen = $state<Screen | null>(null);

  // New item form defaults
  function createBlankItem(categoryId: string = ''): Item {
    return {
      id: `item-${Date.now()}`,
      category_id: categoryId || (categories[0]?.id ?? 'cat-1'),
      name: '',
      description: '',
      price_primary: '',
      price_secondary: '',
      badge: '',
      image_hash: '',
      sold_out: false,
      sort_order: items.length + 1,
    };
  }

  function createBlankCategory(): Category {
    return {
      id: `cat-${Date.now()}`,
      title: '',
      description: '',
      sort_order: categories.length + 1,
      image: null,
    };
  }

  function createBlankPage(): Page {
    return {
      id: `page-${Date.now()}`,
      template_id: templates[0]?.id ?? 'classic-list',
      title: 'New Page',
      duration_ms: 10000,
      transition: 'crossfade',
      transition_ms: 600,
      repeater_capacity: 8,
      schedule_start_min: 0,
      schedule_end_min: 1440,
      category_id: categories[0]?.id ?? null,
      sort_order: pages.length + 1,
    };
  }

  async function loadAllData() {
    loading = true;
    errorMessage = null;
    try {
      const [s, c, i, t, p, sc] = await Promise.all([
        api.getStatus().catch(() => null),
        api.getCategories().catch(() => []),
        api.getItems().catch(() => []),
        api.getTemplates().catch(() => []),
        api.getPages().catch(() => []),
        api.getScreens().catch(() => []),
      ]);
      status = s;
      categories = c;
      items = i;
      templates = t;
      pages = p;
      screens = sc;
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadAllData();
  });

  function showFeedback(msg: string) {
    feedbackMessage = msg;
    setTimeout(() => {
      if (feedbackMessage === msg) {
        feedbackMessage = null;
      }
    }, 4000);
  }

  async function handleToggleSoldOut(item: Item) {
    const original = item.sold_out;
    item.sold_out = !original;
    try {
      await api.setItemSoldOut(item.id, item.sold_out);
      showFeedback(`"${item.name}" marked as ${item.sold_out ? 'SOLD OUT' : 'Available'}`);
    } catch (err: unknown) {
      item.sold_out = original;
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handlePublish() {
    try {
      const res = await api.publish();
      showFeedback(`Published successfully! Snapshot: ${res.hash.substring(0, 8)}`);
      status = await api.getStatus().catch(() => status);
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSaveCategory(e: Event) {
    e.preventDefault();
    if (!editingCategory) return;
    try {
      await api.saveCategory(editingCategory);
      showFeedback(`Saved category "${editingCategory.title}"`);
      editingCategory = null;
      categories = await api.getCategories();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleDeleteCategory(id: string) {
    if (!confirm('Are you sure you want to delete this category?')) return;
    try {
      await api.deleteCategory(id);
      showFeedback('Category deleted');
      categories = await api.getCategories();
      items = await api.getItems();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSaveItem(e: Event) {
    e.preventDefault();
    if (!editingItem) return;
    try {
      await api.saveItem(editingItem);
      showFeedback(`Saved item "${editingItem.name}"`);
      editingItem = null;
      items = await api.getItems();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleDeleteItem(id: string) {
    if (!confirm('Are you sure you want to delete this item?')) return;
    try {
      await api.deleteItem(id);
      showFeedback('Item deleted');
      items = await api.getItems();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSavePage(e: Event) {
    e.preventDefault();
    if (!editingPage) return;
    try {
      await api.savePage(editingPage);
      showFeedback(`Saved page "${editingPage.title}"`);
      editingPage = null;
      pages = await api.getPages();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleDeletePage(id: string) {
    if (!confirm('Are you sure you want to delete this page?')) return;
    try {
      await api.deletePage(id);
      showFeedback('Page deleted');
      pages = await api.getPages();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSaveScreen(e: Event) {
    e.preventDefault();
    if (!editingScreen) return;
    try {
      await api.saveScreen(editingScreen);
      showFeedback(`Saved screen "${editingScreen.name}"`);
      editingScreen = null;
      screens = await api.getScreens();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    }
  }

  const filteredItems = $derived(
    items.filter((item) => {
      const q = filterText.toLowerCase().trim();
      if (!q) return true;
      return (
        item.name.toLowerCase().includes(q) ||
        item.description.toLowerCase().includes(q) ||
        item.badge.toLowerCase().includes(q)
      );
    }),
  );

  function getCategoryTitle(catId: string | null): string {
    if (!catId) return 'All Categories';
    const cat = categories.find((c) => c.id === catId);
    return cat ? cat.title : catId;
  }
</script>

<div class="app-layout">
  <header class="app-header">
    <div class="header-brand">
      <span class="header-icon" aria-hidden="true">📋</span>
      <h1>Chalkboard</h1>
      <span class="badge-status">Single-Node</span>
    </div>

    <div class="header-actions">
      {#if status}
        <span class="status-indicator" title="Snapshot: {status.current_snapshot}">
          <span class="status-dot"></span>
          v{status.version}
        </span>
      {/if}
      <button type="button" class="btn btn-primary" onclick={handlePublish}>
        Publish to Screen
      </button>
    </div>
  </header>

  {#if feedbackMessage}
    <div class="alert alert-success" role="status" aria-live="polite">
      {feedbackMessage}
    </div>
  {/if}

  {#if errorMessage}
    <div class="alert alert-error" role="alert">
      {errorMessage}
      <button
        type="button"
        class="btn-close"
        onclick={() => (errorMessage = null)}
        aria-label="Dismiss error">×</button
      >
    </div>
  {/if}

  <nav class="nav-tabs" aria-label="Main navigation">
    <button
      type="button"
      class="tab-link"
      class:active={currentTab === 'sold-out'}
      onclick={() => (currentTab = 'sold-out')}
    >
      ⚡ Sold-Out Quick Toggle
    </button>
    <button
      type="button"
      class="tab-link"
      class:active={currentTab === 'menu'}
      onclick={() => (currentTab = 'menu')}
    >
      🍲 Menu Editor
    </button>
    <button
      type="button"
      class="tab-link"
      class:active={currentTab === 'templates'}
      onclick={() => (currentTab = 'templates')}
    >
      🎨 Templates
    </button>
    <button
      type="button"
      class="tab-link"
      class:active={currentTab === 'screens'}
      onclick={() => (currentTab = 'screens')}
    >
      🖥️ Screens & Playlist
    </button>
  </nav>

  <main class="app-main">
    {#if loading}
      <div class="loading-state">
        <p>Loading Chalkboard data...</p>
      </div>
    {:else if currentTab === 'sold-out'}
      <!-- TAB 1: FAST SOLD-OUT TOGGLE -->
      <section class="tab-content" aria-labelledby="sold-out-heading">
        <div class="section-header">
          <div>
            <h2 id="sold-out-heading">Sold-Out Fast Toggle</h2>
            <p class="section-sub">Quickly toggle availability for items during active shifts.</p>
          </div>
          <div class="search-box">
            <label for="filter-items" class="visually-hidden">Search items</label>
            <input
              id="filter-items"
              type="search"
              placeholder="Filter items..."
              autocomplete="off"
              bind:value={filterText}
            />
          </div>
        </div>

        {#if filteredItems.length === 0}
          <p class="empty-state">No menu items match your search.</p>
        {:else}
          <div class="sold-out-grid">
            {#each filteredItems as item (item.id)}
              <div class="sold-out-card" class:is-sold-out={item.sold_out}>
                <div class="card-details">
                  <div class="card-title-row">
                    <h3>{item.name}</h3>
                    {#if item.badge}
                      <span class="item-badge">{item.badge}</span>
                    {/if}
                  </div>
                  <p class="card-category">{getCategoryTitle(item.category_id)}</p>
                  <p class="card-price">{item.price_primary}</p>
                </div>

                <div class="toggle-container">
                  <span id={`status-label-${item.id}`} class="toggle-status-text">
                    {item.sold_out ? 'SOLD OUT' : 'AVAILABLE'}
                  </span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={item.sold_out}
                    aria-labelledby={`status-label-${item.id}`}
                    class="switch"
                    onclick={() => handleToggleSoldOut(item)}
                    onkeydown={(e) => {
                      if (e.key === ' ' || e.key === 'Enter') {
                        e.preventDefault();
                        void handleToggleSoldOut(item);
                      }
                    }}
                  >
                    <span class="switch-handle"></span>
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {:else if currentTab === 'menu'}
      <!-- TAB 2: MENU EDITOR -->
      <section class="tab-content" aria-labelledby="menu-heading">
        <div class="section-header">
          <div>
            <h2 id="menu-heading">Menu Editor</h2>
            <p class="section-sub">Manage categories, dishes, descriptions, and pricing.</p>
          </div>
          <div class="button-group">
            <button
              type="button"
              class="btn btn-secondary"
              onclick={() => (editingCategory = createBlankCategory())}
            >
              + Add Category
            </button>
            <button
              type="button"
              class="btn btn-primary"
              onclick={() => (editingItem = createBlankItem())}
            >
              + Add Item
            </button>
          </div>
        </div>

        <!-- Category Edit Modal / Drawer -->
        {#if editingCategory}
          <div
            class="modal-backdrop"
            role="dialog"
            aria-modal="true"
            aria-labelledby="cat-modal-title"
          >
            <div class="modal-dialog">
              <form onsubmit={handleSaveCategory} class="standard-form">
                <fieldset>
                  <legend id="cat-modal-title">
                    {editingCategory.title ? 'Edit Category' : 'Create Category'}
                  </legend>

                  <div class="form-group">
                    <label for="cat-title">Category Title</label>
                    <input
                      id="cat-title"
                      type="text"
                      required
                      autocomplete="off"
                      bind:value={editingCategory.title}
                    />
                  </div>

                  <div class="form-group">
                    <label for="cat-desc">Description</label>
                    <textarea id="cat-desc" rows="2" bind:value={editingCategory.description}
                    ></textarea>
                  </div>

                  <div class="form-group">
                    <label for="cat-order">Sort Order</label>
                    <input id="cat-order" type="number" bind:value={editingCategory.sort_order} />
                  </div>

                  <div class="form-actions">
                    <button
                      type="button"
                      class="btn btn-secondary"
                      onclick={() => (editingCategory = null)}
                    >
                      Cancel
                    </button>
                    <button type="submit" class="btn btn-primary">Save Category</button>
                  </div>
                </fieldset>
              </form>
            </div>
          </div>
        {/if}

        <!-- Item Edit Modal / Drawer -->
        {#if editingItem}
          <div
            class="modal-backdrop"
            role="dialog"
            aria-modal="true"
            aria-labelledby="item-modal-title"
          >
            <div class="modal-dialog">
              <form onsubmit={handleSaveItem} class="standard-form">
                <fieldset>
                  <legend id="item-modal-title">
                    {editingItem.name ? 'Edit Item' : 'Create Item'}
                  </legend>

                  <div class="form-group">
                    <label for="item-name">Item Name</label>
                    <input
                      id="item-name"
                      type="text"
                      required
                      autocomplete="off"
                      bind:value={editingItem.name}
                    />
                  </div>

                  <div class="form-group">
                    <label for="item-category">Category</label>
                    <select id="item-category" bind:value={editingItem.category_id} required>
                      {#each categories as cat (cat.id)}
                        <option value={cat.id}>{cat.title}</option>
                      {/each}
                    </select>
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="item-price">Primary Price</label>
                      <input
                        id="item-price"
                        type="text"
                        placeholder="e.g. 149 kr"
                        required
                        bind:value={editingItem.price_primary}
                      />
                    </div>
                    <div class="form-group">
                      <label for="item-price-sec">Secondary Price</label>
                      <input
                        id="item-price-sec"
                        type="text"
                        placeholder="e.g. Half 95 kr"
                        bind:value={editingItem.price_secondary}
                      />
                    </div>
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="item-badge">Badge (e.g. Vegan, Chef's Choice)</label>
                      <input
                        id="item-badge"
                        type="text"
                        placeholder="Optional badge"
                        bind:value={editingItem.badge}
                      />
                    </div>
                    <div class="form-group">
                      <label for="item-order">Sort Order</label>
                      <input id="item-order" type="number" bind:value={editingItem.sort_order} />
                    </div>
                  </div>

                  <div class="form-group">
                    <label for="item-desc">Description</label>
                    <textarea id="item-desc" rows="3" bind:value={editingItem.description}
                    ></textarea>
                  </div>

                  <div class="form-actions">
                    <button
                      type="button"
                      class="btn btn-secondary"
                      onclick={() => (editingItem = null)}
                    >
                      Cancel
                    </button>
                    <button type="submit" class="btn btn-primary">Save Item</button>
                  </div>
                </fieldset>
              </form>
            </div>
          </div>
        {/if}

        <div class="categories-list">
          {#each categories as cat (cat.id)}
            <div class="category-block">
              <div class="category-header">
                <div>
                  <h3>{cat.title}</h3>
                  {#if cat.description}
                    <p class="category-desc">{cat.description}</p>
                  {/if}
                </div>
                <div class="category-ctrls">
                  <button
                    type="button"
                    class="btn btn-sm"
                    onclick={() => (editingCategory = { ...cat })}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    class="btn btn-sm btn-danger"
                    onclick={() => handleDeleteCategory(cat.id)}
                  >
                    Delete
                  </button>
                </div>
              </div>

              <div class="items-table-wrapper">
                <table class="data-table">
                  <thead>
                    <tr>
                      <th scope="col">Name</th>
                      <th scope="col">Description</th>
                      <th scope="col">Price</th>
                      <th scope="col">Badge</th>
                      <th scope="col">Status</th>
                      <th scope="col" class="text-right">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each items.filter((i) => i.category_id === cat.id) as item (item.id)}
                      <tr class:row-sold-out={item.sold_out}>
                        <td class="font-medium">{item.name}</td>
                        <td class="text-muted">{item.description}</td>
                        <td>{item.price_primary}</td>
                        <td>
                          {#if item.badge}
                            <span class="item-badge">{item.badge}</span>
                          {/if}
                        </td>
                        <td>
                          <span class="status-pill" class:pill-danger={item.sold_out}>
                            {item.sold_out ? 'Sold Out' : 'Active'}
                          </span>
                        </td>
                        <td class="text-right">
                          <button
                            type="button"
                            class="btn btn-sm"
                            onclick={() => (editingItem = { ...item })}
                          >
                            Edit
                          </button>
                          <button
                            type="button"
                            class="btn btn-sm btn-danger"
                            onclick={() => handleDeleteItem(item.id)}
                          >
                            Delete
                          </button>
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            </div>
          {/each}
        </div>
      </section>
    {:else if currentTab === 'templates'}
      <!-- TAB 3: TEMPLATES PICKER -->
      <section class="tab-content" aria-labelledby="templates-heading">
        <div class="section-header">
          <div>
            <h2 id="templates-heading">Template Library</h2>
            <p class="section-sub">
              Pre-designed layouts optimized for crisp legibility on digital menu boards.
            </p>
          </div>
        </div>

        <div class="templates-grid">
          {#each templates as tpl (tpl.id)}
            <article class="template-card">
              <div class="template-preview-mockup">
                <div class="mockup-frame">
                  {#if tpl.id === 'classic-list'}
                    <div class="mockup-classic">
                      <div class="mockup-header-bar"></div>
                      <div class="mockup-line"></div>
                      <div class="mockup-line"></div>
                      <div class="mockup-line"></div>
                    </div>
                  {:else if tpl.id === 'two-column'}
                    <div class="mockup-two-col">
                      <div class="col">
                        <div class="mockup-line"></div>
                        <div class="mockup-line"></div>
                      </div>
                      <div class="col">
                        <div class="mockup-line"></div>
                        <div class="mockup-line"></div>
                      </div>
                    </div>
                  {:else if tpl.id === 'grid-images'}
                    <div class="mockup-grid">
                      <div class="mockup-tile"></div>
                      <div class="mockup-tile"></div>
                      <div class="mockup-tile"></div>
                      <div class="mockup-tile"></div>
                    </div>
                  {:else}
                    <div class="mockup-highlight">
                      <div class="mockup-hero"></div>
                      <div class="mockup-line"></div>
                    </div>
                  {/if}
                </div>
              </div>
              <div class="template-info">
                <h3>{tpl.name}</h3>
                <p class="template-id">ID: <code>{tpl.id}</code></p>
                <p class="template-version">Schema: v{tpl.schema_version}</p>
              </div>
            </article>
          {/each}
        </div>
      </section>
    {:else if currentTab === 'screens'}
      <!-- TAB 4: SCREENS & PLAYLIST -->
      <section class="tab-content" aria-labelledby="screens-heading">
        <div class="section-header">
          <div>
            <h2 id="screens-heading">Screens & Playlist</h2>
            <p class="section-sub">Configure screen rotation, dimensions, and playlist timing.</p>
          </div>
          <button
            type="button"
            class="btn btn-primary"
            onclick={() => (editingPage = createBlankPage())}
          >
            + Add Playlist Page
          </button>
        </div>

        <!-- Page Edit Modal -->
        {#if editingPage}
          <div
            class="modal-backdrop"
            role="dialog"
            aria-modal="true"
            aria-labelledby="page-modal-title"
          >
            <div class="modal-dialog">
              <form onsubmit={handleSavePage} class="standard-form">
                <fieldset>
                  <legend id="page-modal-title">
                    {editingPage.title ? 'Edit Page' : 'Create Page'}
                  </legend>

                  <div class="form-group">
                    <label for="page-title">Page Title</label>
                    <input
                      id="page-title"
                      type="text"
                      required
                      autocomplete="off"
                      bind:value={editingPage.title}
                    />
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="page-template">Template</label>
                      <select id="page-template" bind:value={editingPage.template_id} required>
                        {#each templates as tpl (tpl.id)}
                          <option value={tpl.id}>{tpl.name}</option>
                        {/each}
                      </select>
                    </div>

                    <div class="form-group">
                      <label for="page-category">Category</label>
                      <select id="page-category" bind:value={editingPage.category_id}>
                        <option value={null}>-- All Categories --</option>
                        {#each categories as cat (cat.id)}
                          <option value={cat.id}>{cat.title}</option>
                        {/each}
                      </select>
                    </div>
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="page-duration">Duration (ms)</label>
                      <input
                        id="page-duration"
                        type="number"
                        step="1000"
                        min="1000"
                        required
                        bind:value={editingPage.duration_ms}
                      />
                    </div>

                    <div class="form-group">
                      <label for="page-transition">Transition Effect</label>
                      <select id="page-transition" bind:value={editingPage.transition}>
                        <option value="cut">Cut</option>
                        <option value="crossfade">Crossfade</option>
                        <option value="slide-left">Slide Left</option>
                      </select>
                    </div>
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="page-transition-ms">Transition Duration (ms)</label>
                      <input
                        id="page-transition-ms"
                        type="number"
                        step="100"
                        min="0"
                        bind:value={editingPage.transition_ms}
                      />
                    </div>

                    <div class="form-group">
                      <label for="page-order">Playlist Sort Order</label>
                      <input id="page-order" type="number" bind:value={editingPage.sort_order} />
                    </div>
                  </div>

                  <div class="form-actions">
                    <button
                      type="button"
                      class="btn btn-secondary"
                      onclick={() => (editingPage = null)}
                    >
                      Cancel
                    </button>
                    <button type="submit" class="btn btn-primary">Save Page</button>
                  </div>
                </fieldset>
              </form>
            </div>
          </div>
        {/if}

        <!-- Screen Edit Modal -->
        {#if editingScreen}
          <div
            class="modal-backdrop"
            role="dialog"
            aria-modal="true"
            aria-labelledby="screen-modal-title"
          >
            <div class="modal-dialog">
              <form onsubmit={handleSaveScreen} class="standard-form">
                <fieldset>
                  <legend id="screen-modal-title">Edit Screen: {editingScreen.name}</legend>

                  <div class="form-group">
                    <label for="screen-name">Screen Name</label>
                    <input id="screen-name" type="text" required bind:value={editingScreen.name} />
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="screen-rotation">Rotation (Degrees)</label>
                      <select id="screen-rotation" bind:value={editingScreen.rotation}>
                        <option value={0}>0° (Landscape Standard)</option>
                        <option value={90}>90° (Portrait Right)</option>
                        <option value={180}>180° (Inverted)</option>
                        <option value={270}>270° (Portrait Left)</option>
                      </select>
                    </div>

                    <div class="form-group">
                      <label for="screen-sold-mode">Sold Out Display Mode</label>
                      <select id="screen-sold-mode" bind:value={editingScreen.sold_out_mode}>
                        <option value="badge">Badge</option>
                        <option value="dim">Dim & Badge</option>
                        <option value="hide">Hide Completely</option>
                      </select>
                    </div>
                  </div>

                  <div class="form-row">
                    <div class="form-group">
                      <label for="screen-width">Logical Width (px)</label>
                      <input
                        id="screen-width"
                        type="number"
                        required
                        bind:value={editingScreen.logical_width}
                      />
                    </div>

                    <div class="form-group">
                      <label for="screen-height">Logical Height (px)</label>
                      <input
                        id="screen-height"
                        type="number"
                        required
                        bind:value={editingScreen.logical_height}
                      />
                    </div>
                  </div>

                  <div class="form-actions">
                    <button
                      type="button"
                      class="btn btn-secondary"
                      onclick={() => (editingScreen = null)}
                    >
                      Cancel
                    </button>
                    <button type="submit" class="btn btn-primary">Save Screen Settings</button>
                  </div>
                </fieldset>
              </form>
            </div>
          </div>
        {/if}

        <div class="screens-overview">
          <h3>Connected Displays</h3>
          <div class="screens-grid">
            {#each screens as scr (scr.id)}
              <div class="screen-card">
                <div class="screen-meta">
                  <h4>{scr.name}</h4>
                  <p class="screen-res">{scr.logical_width} × {scr.logical_height} px</p>
                  <p class="screen-rot">Orientation: <strong>{scr.rotation}°</strong></p>
                  <p class="screen-mode">Sold-out mode: <code>{scr.sold_out_mode}</code></p>
                </div>
                <div class="screen-actions">
                  <button
                    type="button"
                    class="btn btn-sm btn-secondary"
                    onclick={() => (editingScreen = { ...scr })}
                  >
                    Configure Screen
                  </button>
                </div>
              </div>
            {/each}
          </div>

          <h3 class="section-divider-title">Playlist Sequence</h3>
          <div class="playlist-table-wrapper">
            <table class="data-table">
              <thead>
                <tr>
                  <th scope="col">Order</th>
                  <th scope="col">Page Title</th>
                  <th scope="col">Template</th>
                  <th scope="col">Duration</th>
                  <th scope="col">Transition</th>
                  <th scope="col">Category</th>
                  <th scope="col" class="text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {#each pages as page (page.id)}
                  <tr>
                    <td><strong>#{page.sort_order}</strong></td>
                    <td class="font-medium">{page.title}</td>
                    <td><code>{page.template_id}</code></td>
                    <td>{(page.duration_ms / 1000).toFixed(1)}s</td>
                    <td>{page.transition} ({page.transition_ms}ms)</td>
                    <td>{getCategoryTitle(page.category_id)}</td>
                    <td class="text-right">
                      <button
                        type="button"
                        class="btn btn-sm"
                        onclick={() => (editingPage = { ...page })}
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        class="btn btn-sm btn-danger"
                        onclick={() => handleDeletePage(page.id)}
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>
      </section>
    {/if}
  </main>
</div>

<style>
  :global(:root) {
    --bg-page: #f8fafc;
    --bg-surface: #ffffff;
    --border-color: #e2e8f0;
    --text-primary: #0f172a;
    --text-muted: #64748b;
    --primary: #2563eb;
    --primary-hover: #1d4ed8;
    --danger: #dc2626;
    --danger-hover: #b91c1c;
    --success: #16a34a;
    --warning: #f59e0b;
    --radius: 8px;
  }

  .app-layout {
    font-family:
      -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    color: var(--text-primary);
    background-color: var(--bg-page);
    min-block-size: 100vh;
    display: flex;
    flex-direction: column;
  }

  /* Header */
  .app-header {
    background-color: var(--bg-surface);
    border-bottom: 1px solid var(--border-color);
    padding-inline: 1.5rem;
    padding-block: 0.75rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header-brand {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .header-brand h1 {
    font-size: 1.25rem;
    font-weight: 700;
    margin: 0;
  }

  .header-icon {
    font-size: 1.5rem;
  }

  .badge-status {
    font-size: 0.75rem;
    font-weight: 600;
    background-color: #e0f2fe;
    color: #0369a1;
    padding-inline: 0.5rem;
    padding-block: 0.2rem;
    border-radius: 9999px;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .status-indicator {
    font-size: 0.85rem;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .status-dot {
    inline-size: 8px;
    block-size: 8px;
    border-radius: 50%;
    background-color: var(--success);
  }

  /* Tabs navigation */
  .nav-tabs {
    display: flex;
    gap: 0.5rem;
    background-color: var(--bg-surface);
    border-bottom: 1px solid var(--border-color);
    padding-inline: 1.5rem;
    padding-block: 0.25rem;
  }

  .tab-link {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding-inline: 1rem;
    padding-block: 0.75rem;
    font-size: 0.95rem;
    font-weight: 500;
    color: var(--text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tab-link:hover {
    color: var(--text-primary);
  }

  .tab-link.active {
    color: var(--primary);
    border-bottom-color: var(--primary);
  }

  /* Main body */
  .app-main {
    flex: 1;
    padding-inline: 1.5rem;
    padding-block: 1.5rem;
    max-inline-size: 1200px;
    margin-inline: auto;
    inline-size: 100%;
    box-sizing: border-box;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-block-end: 1.5rem;
    gap: 1rem;
  }

  .section-header h2 {
    font-size: 1.5rem;
    margin: 0 0 0.25rem 0;
  }

  .section-sub {
    color: var(--text-muted);
    font-size: 0.9rem;
    margin: 0;
  }

  .section-divider-title {
    margin-block-start: 2rem;
    margin-block-end: 1rem;
    font-size: 1.2rem;
  }

  /* Buttons */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding-inline: 1rem;
    padding-block: 0.5rem;
    font-size: 0.9rem;
    font-weight: 500;
    border-radius: var(--radius);
    border: 1px solid transparent;
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      border-color 0.15s ease;
  }

  .btn-primary {
    background-color: var(--primary);
    color: #ffffff;
  }

  .btn-primary:hover {
    background-color: var(--primary-hover);
  }

  .btn-secondary {
    background-color: #ffffff;
    border-color: var(--border-color);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
    background-color: #f1f5f9;
  }

  .btn-sm {
    padding-inline: 0.6rem;
    padding-block: 0.25rem;
    font-size: 0.8rem;
    background-color: #ffffff;
    border: 1px solid var(--border-color);
  }

  .btn-danger {
    color: var(--danger);
  }

  .btn-danger:hover {
    background-color: #fef2f2;
    border-color: #fecaca;
  }

  .button-group {
    display: flex;
    gap: 0.5rem;
  }

  /* Alerts */
  .alert {
    padding-inline: 1.5rem;
    padding-block: 0.75rem;
    font-size: 0.9rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .alert-success {
    background-color: #ecfdf5;
    color: #065f46;
    border-bottom: 1px solid #a7f3d0;
  }

  .alert-error {
    background-color: #fef2f2;
    color: #991b1b;
    border-bottom: 1px solid #fecaca;
  }

  .btn-close {
    background: none;
    border: none;
    font-size: 1.2rem;
    color: inherit;
    cursor: pointer;
  }

  /* Search */
  .search-box input {
    padding-inline: 0.85rem;
    padding-block: 0.45rem;
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    font-size: 0.9rem;
    inline-size: 240px;
  }

  .visually-hidden {
    position: absolute;
    inline-size: 1px;
    block-size: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  /* Sold-Out Toggle Cards */
  .sold-out-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 1rem;
  }

  .sold-out-card {
    background-color: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    padding: 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    transition:
      border-color 0.2s,
      background-color 0.2s;
  }

  .sold-out-card.is-sold-out {
    background-color: #fff1f2;
    border-color: #fecdd3;
  }

  .card-details h3 {
    margin: 0;
    font-size: 1.05rem;
  }

  .card-title-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .card-category {
    margin: 0.2rem 0;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .card-price {
    margin: 0.25rem 0 0 0;
    font-weight: 600;
    font-size: 0.95rem;
  }

  .item-badge {
    font-size: 0.7rem;
    font-weight: 600;
    padding-inline: 0.4rem;
    padding-block: 0.1rem;
    background-color: #fef3c7;
    color: #92400e;
    border-radius: 4px;
  }

  .toggle-container {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 0.4rem;
  }

  .toggle-status-text {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--text-muted);
  }

  .is-sold-out .toggle-status-text {
    color: var(--danger);
  }

  /* Accessible Switch Button */
  .switch {
    inline-size: 48px;
    block-size: 26px;
    border-radius: 9999px;
    background-color: #e2e8f0;
    border: none;
    position: relative;
    cursor: pointer;
    padding: 2px;
    transition: background-color 0.2s ease;
  }

  .switch[aria-checked='true'] {
    background-color: var(--danger);
  }

  .switch-handle {
    display: block;
    inline-size: 22px;
    block-size: 22px;
    background-color: #ffffff;
    border-radius: 50%;
    transition: transform 0.2s ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }

  .switch[aria-checked='true'] .switch-handle {
    transform: translateX(22px);
  }

  /* Categories & Tables */
  .category-block {
    background-color: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    margin-block-end: 1.5rem;
    overflow: hidden;
  }

  .category-header {
    padding-inline: 1.25rem;
    padding-block: 1rem;
    background-color: #f8fafc;
    border-bottom: 1px solid var(--border-color);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .category-header h3 {
    margin: 0;
    font-size: 1.15rem;
  }

  .category-desc {
    margin: 0.25rem 0 0 0;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .items-table-wrapper,
  .playlist-table-wrapper {
    overflow-x: auto;
  }

  .data-table {
    inline-size: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
    text-align: left;
  }

  .data-table th,
  .data-table td {
    padding-inline: 1.25rem;
    padding-block: 0.75rem;
    border-bottom: 1px solid var(--border-color);
  }

  .data-table th {
    background-color: #f8fafc;
    font-weight: 600;
    color: var(--text-muted);
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .row-sold-out {
    background-color: #fff1f2;
    text-decoration: line-through;
    opacity: 0.75;
  }

  .row-sold-out .status-pill {
    text-decoration: none;
  }

  .font-medium {
    font-weight: 500;
  }

  .text-muted {
    color: var(--text-muted);
  }

  .text-right {
    text-align: right;
  }

  .status-pill {
    display: inline-block;
    padding-inline: 0.5rem;
    padding-block: 0.15rem;
    font-size: 0.75rem;
    border-radius: 9999px;
    background-color: #dcfce7;
    color: #166534;
    font-weight: 600;
  }

  .status-pill.pill-danger {
    background-color: #fee2e2;
    color: #991b1b;
  }

  /* Templates tab */
  .templates-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 1.5rem;
  }

  .template-card {
    background-color: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .template-preview-mockup {
    block-size: 160px;
    background-color: #1e293b;
    padding: 1rem;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .mockup-frame {
    inline-size: 100%;
    block-size: 100%;
    background-color: #0f172a;
    border-radius: 4px;
    border: 1px solid #334155;
    padding: 0.5rem;
  }

  .mockup-header-bar {
    inline-size: 40%;
    block-size: 6px;
    background-color: #38bdf8;
    margin-block-end: 0.75rem;
    border-radius: 2px;
  }

  .mockup-line {
    inline-size: 100%;
    block-size: 4px;
    background-color: #475569;
    margin-block-end: 0.4rem;
    border-radius: 2px;
  }

  .mockup-two-col {
    display: flex;
    gap: 0.5rem;
  }

  .mockup-two-col .col {
    flex: 1;
  }

  .mockup-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem;
  }

  .mockup-tile {
    block-size: 40px;
    background-color: #334155;
    border-radius: 2px;
  }

  .mockup-hero {
    inline-size: 100%;
    block-size: 50px;
    background-color: #38bdf8;
    border-radius: 2px;
    margin-block-end: 0.5rem;
  }

  .template-info {
    padding: 1rem;
  }

  .template-info h3 {
    margin: 0 0 0.25rem 0;
    font-size: 1rem;
  }

  .template-id,
  .template-version {
    font-size: 0.8rem;
    color: var(--text-muted);
    margin: 0.2rem 0;
  }

  /* Screens & Playlist */
  .screens-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 1rem;
  }

  .screen-card {
    background-color: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    padding: 1.25rem;
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }

  .screen-meta h4 {
    margin: 0 0 0.25rem 0;
    font-size: 1.1rem;
  }

  .screen-res,
  .screen-rot,
  .screen-mode {
    margin: 0.2rem 0;
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  /* Modals and Forms */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background-color: rgba(15, 23, 42, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 1rem;
  }

  .modal-dialog {
    background-color: var(--bg-surface);
    border-radius: var(--radius);
    inline-size: 100%;
    max-inline-size: 520px;
    box-shadow:
      0 20px 25px -5px rgba(0, 0, 0, 0.1),
      0 8px 10px -6px rgba(0, 0, 0, 0.1);
  }

  .standard-form fieldset {
    border: none;
    padding: 1.5rem;
    margin: 0;
  }

  .standard-form legend {
    font-size: 1.25rem;
    font-weight: 700;
    margin-block-end: 1rem;
    color: var(--text-primary);
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    margin-block-end: 1rem;
  }

  .form-group label {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .form-group input,
  .form-group select,
  .form-group textarea {
    padding-inline: 0.75rem;
    padding-block: 0.5rem;
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    font-size: 0.9rem;
    font-family: inherit;
    background-color: #ffffff;
  }

  .form-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.75rem;
    margin-block-start: 1.5rem;
  }

  .empty-state,
  .loading-state {
    text-align: center;
    padding-block: 3rem;
    color: var(--text-muted);
  }
</style>
