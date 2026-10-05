/**
 * TanStack Query keys and hooks. Everything from the core flows through here; Zustand holds UI
 * state only. The change-event subscription invalidates the keys an entity affects.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { api, onChanged, type AppError, type AppStatus, type SettingUpdate } from "./ipc";
import { reportError } from "./report";

declare module "@tanstack/react-query" {
  interface Register {
    defaultError: AppError;
  }
}

export const keys = {
  status: ["app_status"] as const,
  settings: ["settings"] as const,
};

/** Which query keys an entity change invalidates. Extended as entities arrive. */
const invalidationMap: Record<string, readonly (readonly string[])[]> = {
  setting: [keys.settings],
};

export function useAppStatus() {
  return useQuery({ queryKey: keys.status, queryFn: api.appStatus, staleTime: Infinity });
}

export function useSettings(enabled: boolean) {
  return useQuery({
    queryKey: keys.settings,
    queryFn: api.getSettings,
    enabled,
    staleTime: Infinity,
  });
}

/** A mutation whose result is the new `AppStatus`; the status cache is replaced on success. */
export function useStatusMutation<TVariables>(fn: (variables: TVariables) => Promise<AppStatus>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: (status) => {
      queryClient.setQueryData(keys.status, status);
      if (status.state !== "unlocked") {
        queryClient.removeQueries({ queryKey: keys.settings });
      }
    },
  });
}

export function useUpdateSetting() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: SettingUpdate) => api.updateSetting(input),
    onSuccess: (settings) => {
      queryClient.setQueryData(keys.settings, settings);
    },
  });
}

/** Mount once: route `kept://changed` events into query invalidation. */
export function useChangeSubscription(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    onChanged((entities) => {
      for (const entity of entities) {
        for (const key of invalidationMap[entity] ?? []) {
          void queryClient.invalidateQueries({ queryKey: key });
        }
      }
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((error: unknown) => {
        reportError(error, "subscribing to change events");
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);
}
