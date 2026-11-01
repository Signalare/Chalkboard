// SPDX-License-Identifier: MIT OR Apache-2.0

export interface Category {
  id: string;
  title: string;
  description: string;
  sort_order: number;
  image: string | null;
}

export interface Item {
  id: string;
  category_id: string;
  name: string;
  description: string;
  price_primary: string;
  price_secondary: string;
  badge: string;
  image_hash: string;
  sold_out: boolean;
  sort_order: number;
}

export interface Template {
  id: string;
  name: string;
  schema_version: number;
  document_json: string;
}

export interface Page {
  id: string;
  template_id: string;
  title: string;
  duration_ms: number;
  transition: string;
  transition_ms: number;
  repeater_capacity: number;
  schedule_start_min: number;
  schedule_end_min: number;
  category_id: string | null;
  sort_order: number;
}

export interface Screen {
  id: string;
  name: string;
  node_id: string;
  rotation: number;
  logical_width: number;
  logical_height: number;
  sold_out_mode: string;
  sold_out_badge: string;
  sold_out_dim_alpha: number;
  playlist: string[];
}

export interface NodeStatus {
  status: string;
  version: string;
  current_snapshot: string;
  previous_snapshot: string;
  announcement: {
    text: string;
    visible: boolean;
  };
}

export interface CecAdapterStatus {
  device: string;
  logical_address: number;
  physical_address: string;
  power_status: 'on' | 'standby' | 'transitioning-to-on' | 'transitioning-to-standby' | 'unknown';
  active_source: boolean;
  osd_name: string;
  is_mock: boolean;
}

export interface CecStatus {
  enabled: boolean;
  is_mock: boolean;
  primary_power: string;
  active_source: boolean;
  adapters: CecAdapterStatus[];
}
