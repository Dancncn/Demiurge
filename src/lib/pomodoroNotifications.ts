export interface PomodoroNotificationEvent {
  title: string;
  body: string;
}

export type PomodoroNotificationListener = (
  handler: (event: PomodoroNotificationEvent) => void,
) => Promise<() => void>;

export interface PomodoroNotificationSubscription {
  ready: Promise<void>;
  dispose: () => void;
}

export function showPomodoroNotification(event: PomodoroNotificationEvent) {
  if (typeof Notification === "undefined") return;
  if (Notification.permission === "granted") {
    new Notification(event.title, { body: event.body });
    return;
  }
  if (Notification.permission === "default") {
    void Notification.requestPermission().then((permission) => {
      if (permission === "granted") new Notification(event.title, { body: event.body });
    });
  }
}

export function createPomodoroNotificationSubscription(
  listen: PomodoroNotificationListener,
  present: (event: PomodoroNotificationEvent) => void = showPomodoroNotification,
): PomodoroNotificationSubscription {
  let active = true;
  let unlisten: (() => void) | null = null;
  const ready = listen((event) => {
    if (active) present(event);
  }).then((nextUnlisten) => {
    if (active) unlisten = nextUnlisten;
    else nextUnlisten();
  });

  return {
    ready,
    dispose() {
      if (!active) return;
      active = false;
      unlisten?.();
      unlisten = null;
    },
  };
}
