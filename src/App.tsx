import { useEffect, type ReactElement } from "react";

import { Notices } from "./components/Notices";
import { useAppStatus, useChangeSubscription, useSettings } from "./lib/queries";
import { applyTheme } from "./lib/store";
import { CreateDatabase } from "./screens/CreateDatabase";
import { Setup } from "./screens/Setup";
import { Shell } from "./screens/Shell";
import { Unlock } from "./screens/Unlock";

export function App() {
  useChangeSubscription();
  const status = useAppStatus();
  const unlocked = status.data?.state === "unlocked";
  const settings = useSettings(unlocked);

  useEffect(() => {
    if (settings.data) applyTheme(settings.data.theme);
  }, [settings.data]);

  let screen: ReactElement;
  if (status.isPending) {
    screen = <p className="p-4 text-text-dim">Starting…</p>;
  } else if (status.isError) {
    screen = (
      <p className="p-4 text-negative" role="alert">
        Could not read application status: {status.error.message}
      </p>
    );
  } else {
    switch (status.data.state) {
      case "needs_data_dir":
        screen = <Setup />;
        break;
      case "needs_database":
        screen = <CreateDatabase dataDir={status.data.data_dir ?? ""} />;
        break;
      case "locked":
        screen = <Unlock status={status.data} />;
        break;
      case "unlocked":
        screen = <Shell status={status.data} />;
        break;
    }
  }

  return (
    <div className="h-full bg-bg text-text">
      {screen}
      <Notices />
    </div>
  );
}
