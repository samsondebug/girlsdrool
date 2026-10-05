/**
 * TanStack Query keys and hooks. Everything from the core flows through here; Zustand holds UI
 * state only. The change-event subscription invalidates the keys an entity affects.
 */
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryKey,
} from "@tanstack/react-query";
import { useEffect } from "react";

import {
  api,
  onChanged,
  type AccountPatch,
  type AppError,
  type AppStatus,
  type Cursor,
  type ImportSource,
  type NewAccount,
  type NewCategory,
  type QuarantineAction,
  type SettingUpdate,
  type SplitPart,
  type TxnPatch,
} from "./ipc";
import type { LedgerFilter } from "./query-chips";
import { reportError } from "./report";

declare module "@tanstack/react-query" {
  interface Register {
    defaultError: AppError;
  }
}

export const keys = {
  status: ["app_status"] as const,
  settings: ["settings"] as const,
  accounts: ["accounts"] as const,
  categories: ["categories"] as const,
  profiles: ["import_profiles"] as const,
  batches: ["import_batches"] as const,
  quarantine: ["import_quarantine"] as const,
  savedViews: ["saved_views"] as const,
  ledger: (filter: LedgerFilter) => ["ledger", filter] as const,
  ledgerAll: ["ledger"] as const,
  children: (parentId: number) => ["txn_children", parentId] as const,
};

/** Which query keys an entity change invalidates. */
const invalidationMap: Record<string, readonly QueryKey[]> = {
  setting: [keys.settings],
  account: [keys.accounts, keys.ledgerAll],
  category: [keys.categories, keys.ledgerAll],
  txn: [keys.ledgerAll, ["txn_children"]],
  import_batch: [keys.batches],
  import_quarantine: [keys.quarantine],
  saved_view: [keys.savedViews],
};

const LEDGER_PAGE = 200;

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

export function useAccounts() {
  return useQuery({ queryKey: keys.accounts, queryFn: api.listAccounts, staleTime: Infinity });
}

export function useCategories() {
  return useQuery({ queryKey: keys.categories, queryFn: api.listCategories, staleTime: Infinity });
}

export function useProfiles() {
  return useQuery({
    queryKey: keys.profiles,
    queryFn: api.listImportProfiles,
    staleTime: Infinity,
  });
}

export function useBatches() {
  return useQuery({
    queryKey: keys.batches,
    queryFn: () => api.listImportBatches(50),
    staleTime: Infinity,
  });
}

export function useQuarantine() {
  return useQuery({ queryKey: keys.quarantine, queryFn: api.listQuarantine, staleTime: Infinity });
}

export function useSavedViews() {
  return useQuery({ queryKey: keys.savedViews, queryFn: api.listSavedViews, staleTime: Infinity });
}

/** The ledger as pages the virtualizer pulls in while scrolling. */
export function useLedger(filter: LedgerFilter) {
  return useInfiniteQuery({
    queryKey: keys.ledger(filter),
    queryFn: ({ pageParam }) => api.ledgerQuery(filter, pageParam, LEDGER_PAGE),
    initialPageParam: null as Cursor | null,
    getNextPageParam: (last) => last.next_cursor,
    staleTime: Infinity,
  });
}

export function useChildren(parentId: number | null) {
  return useQuery({
    queryKey: keys.children(parentId ?? 0),
    queryFn: () => api.ledgerChildren(parentId ?? 0),
    enabled: parentId !== null,
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
        queryClient.removeQueries({ queryKey: keys.ledgerAll });
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

export function useCreateAccount() {
  return useMutation({ mutationFn: (account: NewAccount) => api.createAccount(account) });
}

export function useUpdateAccount() {
  return useMutation({
    mutationFn: (input: { id: number; patch: AccountPatch }) =>
      api.updateAccount(input.id, input.patch),
  });
}

export function useCreateCategory() {
  return useMutation({ mutationFn: (category: NewCategory) => api.createCategory(category) });
}

export function useImportPreview() {
  return useMutation({
    mutationFn: (input: { accountId: number; profileId: number | null; source: ImportSource }) =>
      api.importPreview(input.accountId, input.profileId, input.source),
  });
}

export function useImportCommit() {
  return useMutation({
    mutationFn: (input: { accountId: number; profileId: number | null; source: ImportSource }) =>
      api.importCommit(input.accountId, input.profileId, input.source),
  });
}

export function useUndoBatch() {
  return useMutation({ mutationFn: (batchId: number) => api.undoImportBatch(batchId) });
}

export function useResolveQuarantine() {
  return useMutation({
    mutationFn: (input: { id: number; action: QuarantineAction }) =>
      api.resolveQuarantine(input.id, input.action),
  });
}

export function useUpdateTxn() {
  return useMutation({
    mutationFn: (input: { id: number; patch: TxnPatch }) => api.updateTxn(input.id, input.patch),
  });
}

export function useRecategorize() {
  return useMutation({
    mutationFn: (input: { ids: number[]; categoryId: number | null }) =>
      api.recategorize(input.ids, input.categoryId),
  });
}

export function useSplit() {
  return useMutation({
    mutationFn: (input: { parentId: number; parts: SplitPart[] }) =>
      api.splitTxn(input.parentId, input.parts),
  });
}

export function useUnsplit() {
  return useMutation({ mutationFn: (parentId: number) => api.unsplitTxn(parentId) });
}

export function useSaveView() {
  return useMutation({
    mutationFn: (input: { name: string; queryText: string }) =>
      api.saveView(input.name, input.queryText),
  });
}

export function useDeleteView() {
  return useMutation({ mutationFn: (id: number) => api.deleteSavedView(id) });
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
