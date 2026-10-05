/**
 * A correction proposes a rule; nothing is created until the person says so (ARCHITECTURE §6.4).
 * The proposal says how many other rows it would match today, so a too-broad payee is visible
 * before the rule exists.
 */
import { useCallback } from "react";

import { api, type RuleProposal } from "./ipc";
import { reportError } from "./report";
import { useUiStore } from "./store";

function describe(proposal: RuleProposal, categoryPath: string): string {
  const others =
    proposal.would_match === 0
      ? "no other row today"
      : proposal.would_match === 1
        ? "1 other row today"
        : `${proposal.would_match} other rows today`;
  return `Rule idea: payee contains "${proposal.match_payee_contains}" → ${categoryPath}. It would match ${others}.`;
}

/** After a category correction, offer the rule it suggests as a notice with a "Create rule" action. */
export function useCorrectionProposal(): (txnId: number, categoryPath: string) => void {
  const pushNotice = useUiStore((s) => s.pushNotice);
  return useCallback(
    (txnId: number, categoryPath: string) => {
      api
        .proposeRule(txnId)
        .then((proposal) => {
          if (proposal.match_payee_contains === "" || proposal.action_category_id === null) return;
          pushNotice({
            tone: "info",
            text: describe(proposal, categoryPath),
            action: {
              label: "Create rule",
              run: () => {
                api
                  .createRule({
                    name: proposal.name,
                    match_payee_contains: proposal.match_payee_contains,
                    action_category_id: proposal.action_category_id,
                    action_venture_id: proposal.action_venture_id,
                  })
                  .then((rule) => {
                    pushNotice({
                      tone: "positive",
                      text: `Rule "${rule.name}" created. Rules › Apply rules now runs it over the ledger.`,
                    });
                  })
                  .catch((error: unknown) => {
                    reportError(error, "creating the rule");
                  });
              },
            },
          });
        })
        .catch((error: unknown) => {
          reportError(error, "proposing a rule");
        });
    },
    [pushNotice],
  );
}
