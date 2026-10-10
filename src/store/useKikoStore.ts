import { create } from "zustand";
import type { AgentStatus } from "../types/events";

interface KikoState {
  status: AgentStatus;
  setStatus: (status: AgentStatus) => void;
}

export const useKikoStore = create<KikoState>()((set) => ({
  status: "idle",
  setStatus: (status) => set({ status }),
}));
