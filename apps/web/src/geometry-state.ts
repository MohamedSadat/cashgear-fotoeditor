import { create } from 'zustand';

export const useGeometry = create<{
  showGuides: boolean;
  showRulers: boolean;
  snapping: boolean;
}>(() => ({ showGuides: true, showRulers: false, snapping: true }));
