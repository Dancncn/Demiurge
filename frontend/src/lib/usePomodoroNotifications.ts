import { useEffect } from "react";
import * as api from "@/lib/api";
import { createPomodoroNotificationSubscription } from "@/lib/pomodoroNotifications";

// Mount once at the application root so card visibility cannot disable alerts.
export function usePomodoroNotifications() {
  useEffect(() => {
    const subscription = createPomodoroNotificationSubscription(api.listenPomodoroCompleted);
    void subscription.ready.catch((error) => {
      console.error("Failed to subscribe to pomodoro notifications", error);
    });
    return subscription.dispose;
  }, []);
}
