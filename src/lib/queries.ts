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
  type RuleInput,
  type SettingUpdate,
  type SplitPart,
  type TransferKind,
  type EarmarkInput,
  type EntryInput,
  type IncomeInput,
  type ObligationInput,
  type ObligationStatus,
  type ReconInput,
  type TxnPatch,
  type VentureInput,
  type Scenario,
  type DebtInput,
  type DebtPaymentInput,
  type InformalInput,
  type ProfileInput,
  type ProfileSpec,
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
  rules: ["rules"] as const,
  ventures: ["ventures"] as const,
  reviewQueue: ["review_queue"] as const,
  spending: (from: string, to: string) => ["spending_view", from, to] as const,
  spendingAll: ["spending_view"] as const,
  cash: (from: string, to: string) => ["cash_view", from, to] as const,
  cashAll: ["cash_view"] as const,
  views: (from: string, to: string) => ["compare_views", from, to] as const,
  viewsAll: ["compare_views"] as const,
  linkDetails: (txnId: number) => ["link_details", txnId] as const,
  linkDetailsAll: ["link_details"] as const,
  linkCandidates: (txnId: number) => ["link_candidates", txnId] as const,
  linkCandidatesAll: ["link_candidates"] as const,
  reconciliations: (accountId: number) => ["reconciliations", accountId] as const,
  reconciliationsAll: ["reconciliations"] as const,
  explorer: (id: number) => ["difference_explorer", id] as const,
  explorerAll: ["difference_explorer"] as const,
  trust: ["trust_status"] as const,
  incomeStreams: ["income_streams"] as const,
  receipts: (streamId: number) => ["receipts", streamId] as const,
  receiptsAll: ["receipts"] as const,
  obligations: ["obligations"] as const,
  payments: (obligationId: number) => ["payments", obligationId] as const,
  paymentsAll: ["payments"] as const,
  earmarks: ["earmarks"] as const,
  entries: (earmarkId: number) => ["earmark_entries", earmarkId] as const,
  entriesAll: ["earmark_entries"] as const,
  occurrences: (kind: string, id: number) => ["next_occurrences", kind, id] as const,
  occurrencesAll: ["next_occurrences"] as const,
  policies: ["policies"] as const,
  safe: ["safe_to_spend"] as const,
  upcoming: ["upcoming"] as const,
  forecastAll: ["forecast"] as const,
  forecast: (scenario: Scenario) => ["forecast", scenario] as const,
  variableModel: ["variable_spend_model"] as const,
  debts: ["debts"] as const,
  debtPaymentsAll: ["debt_payments"] as const,
  debtPayments: (debtId: number) => ["debt_payments", debtId] as const,
  debtCandidates: (debtId: number) => ["debt_candidates", debtId] as const,
  debtCandidatesAll: ["debt_candidates"] as const,
  informalLoans: ["informal_loans"] as const,
  debtComparisonAll: ["debt_comparison"] as const,
  debtComparison: (extra: number | null) => ["debt_comparison", extra] as const,
  debtTotals: ["debt_totals"] as const,
  ventureSummary: ["venture_summary"] as const,
  currentReview: ["review", "current"] as const,
  reviews: ["review", "all"] as const,
  reviewAll: ["review"] as const,
  snapshots: ["snapshots"] as const,
  trends: ["trends"] as const,
  backups: ["backups"] as const,
};

/** The hero and the upcoming panel read the plan and the ledger; anything there moves them. */
const heroDependent: readonly QueryKey[] = [
  ["safe_to_spend"],
  ["upcoming"],
  ["forecast"],
  ["variable_spend_model"],
  ["debts"],
  ["informal_loans"],
  ["debt_comparison"],
  ["debt_totals"],
  ["venture_summary"],
];

/** Reconciliation periods, the explorer and trust move whenever a ledger row does. */
const reconDependent: readonly QueryKey[] = [
  ["reconciliations"],
  ["difference_explorer"],
  ["trust_status"],
];

/** Everything a changed ledger row can move: rows, totals, the queue, links and both views. */
const rowDependent: readonly QueryKey[] = [
  ["venture_summary"],
  keys.ledgerAll,
  ["txn_children"],
  keys.reviewQueue,
  keys.spendingAll,
  keys.cashAll,
  keys.viewsAll,
  // every write re-matches receipts and payments (ADR-0041 §1)
  keys.receiptsAll,
  keys.paymentsAll,
  keys.occurrencesAll,
  keys.linkDetailsAll,
  keys.linkCandidatesAll,
  ...reconDependent,
  ...heroDependent,
];

/** Which query keys an entity change invalidates. */
const invalidationMap: Record<string, readonly QueryKey[]> = {
  setting: [keys.settings, keys.trust, ...heroDependent],
  account: [
    keys.accounts,
    keys.ledgerAll,
    keys.cashAll,
    keys.viewsAll,
    ...reconDependent,
    ...heroDependent,
  ],
  reconciliation: [...reconDependent, ...heroDependent],
  income_stream: [keys.incomeStreams, keys.receiptsAll, keys.occurrencesAll, ...heroDependent],
  obligation: [keys.obligations, keys.paymentsAll, keys.occurrencesAll, ...heroDependent],
  earmark: [keys.earmarks, keys.entriesAll, ...heroDependent],
  category: [keys.categories, keys.ledgerAll, keys.spendingAll, keys.viewsAll],
  txn: rowDependent,
  transfer_link: rowDependent,
  refund_link: rowDependent,
  firewall_ack: rowDependent,
  import_batch: [keys.batches],
  import_quarantine: [keys.quarantine, keys.explorerAll],
  saved_view: [keys.savedViews],
  rule: [keys.rules],
  venture: [keys.ventures, keys.accounts, keys.ventureSummary],
  variable_spend_override: [keys.variableModel, keys.forecastAll],
  debt: [
    keys.debts,
    keys.informalLoans,
    keys.debtComparisonAll,
    keys.debtCandidatesAll,
    keys.obligations,
    ...heroDependent,
  ],
  debt_payment: [
    keys.debts,
    keys.debtPaymentsAll,
    keys.debtCandidatesAll,
    keys.informalLoans,
    keys.debtComparisonAll,
  ],
  informal_loan: [keys.informalLoans, keys.debts, keys.debtComparisonAll, ...heroDependent],
  review: [keys.reviewAll],
  snapshot: [keys.forecastAll, keys.snapshots, keys.trends, keys.reviewAll],
  import_profile: [keys.profiles],
  backup: [keys.backups],
};

/** A restore replaces the whole database: every cached read is stale. */
const EVERYTHING = "restore";

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
      // Locking drops every cached read: the next unlock may open another folder, and an
      // unlock itself takes the daily snapshot and backup, so nothing cached before it holds.
      queryClient.removeQueries({ predicate: (q) => q.queryKey !== keys.status });
      queryClient.setQueryData(keys.status, status);
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

// ---- rules, links, review, views (M2) -----------------------------------------------------------

export function useRules() {
  return useQuery({ queryKey: keys.rules, queryFn: api.listRules, staleTime: Infinity });
}

export function useVentures() {
  return useQuery({ queryKey: keys.ventures, queryFn: api.listVentures, staleTime: Infinity });
}

export function useReviewQueue() {
  return useQuery({
    queryKey: keys.reviewQueue,
    queryFn: () => api.reviewQueue(500),
    staleTime: Infinity,
  });
}

export function useSpendingView(from: string, to: string, enabled: boolean) {
  return useQuery({
    queryKey: keys.spending(from, to),
    queryFn: () => api.spendingView(from, to),
    enabled,
    staleTime: Infinity,
  });
}

export function useCashView(from: string, to: string, enabled: boolean) {
  return useQuery({
    queryKey: keys.cash(from, to),
    queryFn: () => api.cashView(from, to),
    enabled,
    staleTime: Infinity,
  });
}

export function useViewComparison(from: string, to: string, enabled: boolean) {
  return useQuery({
    queryKey: keys.views(from, to),
    queryFn: () => api.compareViews(from, to),
    enabled,
    staleTime: Infinity,
  });
}

export function useLinkDetails(txnId: number) {
  return useQuery({
    queryKey: keys.linkDetails(txnId),
    queryFn: () => api.linkDetails(txnId),
    staleTime: Infinity,
  });
}

export function useLinkCandidates(txnId: number) {
  return useQuery({
    queryKey: keys.linkCandidates(txnId),
    queryFn: () => api.linkCandidates(txnId),
    staleTime: Infinity,
  });
}

export function useCreateRule() {
  return useMutation({ mutationFn: (input: RuleInput) => api.createRule(input) });
}

export function useUpdateRule() {
  return useMutation({
    mutationFn: (input: { id: number; input: RuleInput }) => api.updateRule(input.id, input.input),
  });
}

export function useDeleteRule() {
  return useMutation({ mutationFn: (id: number) => api.deleteRule(id) });
}

export function useReorderRules() {
  return useMutation({ mutationFn: (ids: number[]) => api.reorderRules(ids) });
}

export function useApplyRules() {
  return useMutation({ mutationFn: () => api.applyRules() });
}

export function useLinkTransfer() {
  return useMutation({
    mutationFn: (input: { outTxnId: number; inTxnId: number; kind: TransferKind | null }) =>
      api.linkTransfer(input.outTxnId, input.inTxnId, input.kind),
  });
}

export function useUnlinkTransfer() {
  return useMutation({ mutationFn: (linkId: number) => api.unlinkTransfer(linkId) });
}

export function useLinkRefund() {
  return useMutation({
    mutationFn: (input: { originalTxnId: number; refundTxnId: number }) =>
      api.linkRefund(input.originalTxnId, input.refundTxnId),
  });
}

export function useUnlinkRefund() {
  return useMutation({ mutationFn: (linkId: number) => api.unlinkRefund(linkId) });
}

export function useAcknowledgeFirewall() {
  return useMutation({
    mutationFn: (input: { txnId: number; note: string }) =>
      api.acknowledgeFirewall(input.txnId, input.note),
  });
}

export function useCreateVenture() {
  return useMutation({ mutationFn: (input: VentureInput) => api.createVenture(input) });
}

export function useUpdateVenture() {
  return useMutation({
    mutationFn: (input: { id: number; input: VentureInput }) =>
      api.updateVenture(input.id, input.input),
  });
}

/** Mount once: route `kept://changed` events into query invalidation. */
export function useChangeSubscription(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    onChanged((entities) => {
      if (entities.includes(EVERYTHING)) {
        void queryClient.invalidateQueries();
        return;
      }
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

// ---- reconciliation and trust (M3) --------------------------------------------------------------

export function useReconciliations(accountId: number | null) {
  return useQuery({
    queryKey: keys.reconciliations(accountId ?? 0),
    queryFn: () => api.listReconciliations(accountId ?? 0),
    enabled: accountId !== null,
    staleTime: Infinity,
  });
}

export function useExplorer(id: number | null) {
  return useQuery({
    queryKey: keys.explorer(id ?? 0),
    queryFn: () => api.differenceExplorer(id ?? 0),
    enabled: id !== null,
    staleTime: Infinity,
  });
}

/** Per-account trust and the hero's trust; every figure's untrusted marking reads this. */
export function useTrust() {
  return useQuery({ queryKey: keys.trust, queryFn: api.trustStatus, staleTime: Infinity });
}

export function useReconcile() {
  return useMutation({ mutationFn: (input: ReconInput) => api.reconcile(input) });
}

export function useDeleteReconciliation() {
  return useMutation({ mutationFn: (id: number) => api.deleteReconciliation(id) });
}

// ---- plan and the hero (M4) -----------------------------------------------------------------------

export function useIncomeStreams() {
  return useQuery({
    queryKey: keys.incomeStreams,
    queryFn: api.listIncomeStreams,
    staleTime: Infinity,
  });
}

export function useReceipts(streamId: number | null) {
  return useQuery({
    queryKey: keys.receipts(streamId ?? 0),
    queryFn: () => api.listReceipts(streamId ?? 0),
    enabled: streamId !== null,
    staleTime: Infinity,
  });
}

export function useObligations() {
  return useQuery({
    queryKey: keys.obligations,
    queryFn: api.listObligations,
    staleTime: Infinity,
  });
}

export function usePayments(obligationId: number | null) {
  return useQuery({
    queryKey: keys.payments(obligationId ?? 0),
    queryFn: () => api.listPayments(obligationId ?? 0),
    enabled: obligationId !== null,
    staleTime: Infinity,
  });
}

export function useEarmarks() {
  return useQuery({ queryKey: keys.earmarks, queryFn: api.listEarmarks, staleTime: Infinity });
}

export function useEarmarkEntries(earmarkId: number | null) {
  return useQuery({
    queryKey: keys.entries(earmarkId ?? 0),
    queryFn: () => api.listEarmarkEntries(earmarkId ?? 0),
    enabled: earmarkId !== null,
    staleTime: Infinity,
  });
}

export function useNextOccurrences(kind: "income" | "obligation", id: number, count = 3) {
  return useQuery({
    queryKey: keys.occurrences(kind, id),
    queryFn: () => api.nextOccurrences(kind, id, count),
    staleTime: Infinity,
  });
}

export function usePolicies() {
  return useQuery({ queryKey: keys.policies, queryFn: api.listPolicies, staleTime: Infinity });
}

export function useSafeToSpend() {
  return useQuery({ queryKey: keys.safe, queryFn: api.safeToSpend, staleTime: Infinity });
}

export function useForecast(scenario: Scenario) {
  return useQuery({
    queryKey: keys.forecast(scenario),
    queryFn: () => api.forecast(scenario),
    staleTime: Infinity,
  });
}

export function useVariableModel() {
  return useQuery({
    queryKey: keys.variableModel,
    queryFn: api.variableSpendModel,
    staleTime: Infinity,
  });
}

export function useSetVariableOverride() {
  return useMutation({
    mutationFn: (input: { categoryId: number; cents: number | null }) =>
      api.setVariableSpendOverride(input.categoryId, input.cents),
  });
}

export function useSaveForecastPlan() {
  return useMutation({ mutationFn: () => api.saveForecastPlan() });
}

export function useCurrentReview() {
  return useQuery({
    queryKey: keys.currentReview,
    queryFn: api.currentReview,
    staleTime: Infinity,
  });
}

export function useReviews() {
  return useQuery({ queryKey: keys.reviews, queryFn: api.listReviews, staleTime: Infinity });
}

export function useTrends() {
  return useQuery({ queryKey: keys.trends, queryFn: api.listTrends, staleTime: Infinity });
}

export function useStartReview() {
  return useMutation({ mutationFn: () => api.startReview() });
}

export function useRefreshReview() {
  return useMutation({ mutationFn: (id: number) => api.refreshReview(id) });
}

export function useSetReviewActions() {
  return useMutation({
    mutationFn: (input: { id: number; actions: string[] }) =>
      api.setReviewActions(input.id, input.actions),
  });
}

export function useCompleteReview() {
  return useMutation({
    mutationFn: (input: { id: number; actions: string[]; notes: string }) =>
      api.completeReview(input.id, input.actions, input.notes),
  });
}

export function useAbandonReview() {
  return useMutation({ mutationFn: (id: number) => api.abandonReview(id) });
}

export function useSetReviewActionDone() {
  return useMutation({
    mutationFn: (input: { actionId: number; done: boolean }) =>
      api.setReviewActionDone(input.actionId, input.done),
  });
}

export function useTakeSnapshot() {
  return useMutation({ mutationFn: () => api.takeSnapshot() });
}

export function useVentureSummary() {
  return useQuery({
    queryKey: keys.ventureSummary,
    queryFn: api.ventureSummary,
    staleTime: Infinity,
  });
}

export function useDebtTotals() {
  return useQuery({ queryKey: keys.debtTotals, queryFn: api.debtTotals, staleTime: Infinity });
}

export function useDebts() {
  return useQuery({ queryKey: keys.debts, queryFn: api.listDebts, staleTime: Infinity });
}

export function useDebtPayments(debtId: number) {
  return useQuery({
    queryKey: keys.debtPayments(debtId),
    queryFn: () => api.listDebtPayments(debtId),
    staleTime: Infinity,
  });
}

export function useDebtPaymentCandidates(debtId: number, enabled: boolean) {
  return useQuery({
    queryKey: keys.debtCandidates(debtId),
    queryFn: () => api.debtPaymentCandidates(debtId),
    enabled,
    staleTime: 0,
  });
}

export function useInformalLoans() {
  return useQuery({
    queryKey: keys.informalLoans,
    queryFn: api.listInformalLoans,
    staleTime: Infinity,
  });
}

export function useDebtComparison(extra: number | null) {
  return useQuery({
    queryKey: keys.debtComparison(extra),
    queryFn: () => api.debtComparison(extra),
    staleTime: Infinity,
  });
}

export function useCreateDebt() {
  return useMutation({ mutationFn: (input: DebtInput) => api.createDebt(input) });
}

export function useUpdateDebt() {
  return useMutation({
    mutationFn: (input: { id: number; input: DebtInput }) => api.updateDebt(input.id, input.input),
  });
}

export function useRecordDebtPayment() {
  return useMutation({
    mutationFn: (input: { debtId: number; input: DebtPaymentInput }) =>
      api.recordDebtPayment(input.debtId, input.input),
  });
}

export function useRemoveDebtPayment() {
  return useMutation({ mutationFn: (id: number) => api.removeDebtPayment(id) });
}

export function useCreateInformalLoan() {
  return useMutation({ mutationFn: (input: InformalInput) => api.createInformalLoan(input) });
}

export function useUpdateInformalLoan() {
  return useMutation({
    mutationFn: (input: { debtId: number; input: InformalInput }) =>
      api.updateInformalLoan(input.debtId, input.input),
  });
}

export function useSetInformalNote() {
  return useMutation({
    mutationFn: (input: { debtId: number; note: string }) =>
      api.setInformalNote(input.debtId, input.note),
  });
}

export function useAddInformalScheduleRow() {
  return useMutation({
    mutationFn: (input: { debtId: number; dueDate: string; amountCents: number }) =>
      api.addInformalScheduleRow(input.debtId, input.dueDate, input.amountCents),
  });
}

export function useDeleteInformalScheduleRow() {
  return useMutation({ mutationFn: (id: number) => api.deleteInformalScheduleRow(id) });
}

export function useUpcoming(days = 14) {
  return useQuery({
    queryKey: keys.upcoming,
    queryFn: () => api.upcoming(days),
    staleTime: Infinity,
  });
}

export function useCreateIncomeStream() {
  return useMutation({ mutationFn: (input: IncomeInput) => api.createIncomeStream(input) });
}

export function useUpdateIncomeStream() {
  return useMutation({
    mutationFn: (input: { id: number; input: IncomeInput }) =>
      api.updateIncomeStream(input.id, input.input),
  });
}

export function useRecordReceipt() {
  return useMutation({
    mutationFn: (input: { streamId: number; dueDate: string; txnId: number }) =>
      api.recordReceipt(input.streamId, input.dueDate, input.txnId),
  });
}

export function useRemoveReceipt() {
  return useMutation({
    mutationFn: (input: { streamId: number; dueDate: string }) =>
      api.removeReceipt(input.streamId, input.dueDate),
  });
}

export function useCreateObligation() {
  return useMutation({ mutationFn: (input: ObligationInput) => api.createObligation(input) });
}

export function useUpdateObligation() {
  return useMutation({
    mutationFn: (input: { id: number; input: ObligationInput }) =>
      api.updateObligation(input.id, input.input),
  });
}

export function useSetObligationStatus() {
  return useMutation({
    mutationFn: (input: { id: number; status: ObligationStatus }) =>
      api.setObligationStatus(input.id, input.status),
  });
}

export function useDeleteObligationCandidate() {
  return useMutation({ mutationFn: (id: number) => api.deleteObligationCandidate(id) });
}

export function useDetectCandidates() {
  return useMutation({ mutationFn: () => api.detectObligationCandidates() });
}

export function useRecordPayment() {
  return useMutation({
    mutationFn: (input: { obligationId: number; dueDate: string; txnId: number }) =>
      api.recordPayment(input.obligationId, input.dueDate, input.txnId),
  });
}

export function useRemovePayment() {
  return useMutation({
    mutationFn: (input: { obligationId: number; dueDate: string }) =>
      api.removePayment(input.obligationId, input.dueDate),
  });
}

export function useCreateEarmark() {
  return useMutation({ mutationFn: (input: EarmarkInput) => api.createEarmark(input) });
}

export function useUpdateEarmark() {
  return useMutation({
    mutationFn: (input: { id: number; input: EarmarkInput }) =>
      api.updateEarmark(input.id, input.input),
  });
}

export function useAddEarmarkEntry() {
  return useMutation({
    mutationFn: (input: { earmarkId: number; input: EntryInput }) =>
      api.addEarmarkEntry(input.earmarkId, input.input),
  });
}

export function useDeleteEarmarkEntry() {
  return useMutation({ mutationFn: (id: number) => api.deleteEarmarkEntry(id) });
}

// ---- institution profiles, backups, restore, exports (M9) -------------------------------------

export function useCreateProfile() {
  return useMutation({ mutationFn: (input: ProfileInput) => api.createImportProfile(input) });
}

export function useUpdateProfile() {
  return useMutation({
    mutationFn: ({ id, input }: { id: number; input: ProfileInput }) =>
      api.updateImportProfile(id, input),
  });
}

export function useDeleteProfile() {
  return useMutation({ mutationFn: (id: number) => api.deleteImportProfile(id) });
}

export function useDraftProfile() {
  return useMutation({ mutationFn: (source: ImportSource) => api.draftImportProfile(source) });
}

export function useTestProfile() {
  return useMutation({
    mutationFn: ({ spec, source }: { spec: ProfileSpec; source: ImportSource }) =>
      api.testImportProfile(spec, source),
  });
}

export function useBackups() {
  return useQuery({ queryKey: keys.backups, queryFn: () => api.listBackups() });
}

export function useBackupNow() {
  return useMutation({ mutationFn: () => api.backupNow() });
}

export function useRestoreStage() {
  return useMutation({
    mutationFn: ({ path, passphrase }: { path: string; passphrase: string }) =>
      api.restoreStage(path, passphrase),
  });
}

export function useRestoreDiscard() {
  return useMutation({ mutationFn: () => api.restoreDiscard() });
}

export function useExportFull() {
  return useMutation({ mutationFn: (dir: string | null) => api.exportFull(dir) });
}

export function useExportAuditPack() {
  return useMutation({ mutationFn: (dir: string | null) => api.exportAuditPack(dir) });
}
