#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <syslog.h>
#include <unistd.h>

#include "core.h"

static volatile sig_atomic_t running = 1;
static volatile sig_atomic_t reload_flag = 0;

static void handle_term(int sig) {
  (void)sig;
  running = 0;
}

static void handle_reload(int sig) {
  (void)sig;
  reload_flag = 1;
}

static void setup_signal_handlers(void) {
  struct sigaction sa;
  memset(&sa, 0, sizeof(sa));

  // SIGTERM: graceful shutdown
  sa.sa_handler = handle_term;
  sigaction(SIGTERM, &sa, NULL);
  sigaction(SIGINT, &sa, NULL);

  // SIGHUP: reload configuration
  sa.sa_handler = handle_reload;
  sigaction(SIGHUP, &sa, NULL);

  // Ignore SIGPIPE
  sa.sa_handler = SIG_IGN;
  sigaction(SIGPIPE, &sa, NULL);
}

int main(void) {
  openlog("firebox-daemon", LOG_PID | LOG_NDELAY, LOG_DAEMON);
  syslog(LOG_INFO, "FireBox daemon starting...");

  setup_signal_handlers();

  // Start the core service
  int32_t ret = fire_box_start();
  if (ret != 0) {
    syslog(LOG_ERR, "Failed to start core service (code: %d)", ret);
    closelog();
    return 1;
  }
  syslog(LOG_INFO, "Core service started");

  // Main event loop
  while (running) {
    if (reload_flag) {
      syslog(LOG_INFO, "Reloading configuration...");
      ret = fire_box_reload();
      if (ret == 0) {
        syslog(LOG_INFO, "Configuration reloaded");
      } else {
        syslog(LOG_WARNING, "Reload failed (code: %d)", ret);
      }
      reload_flag = 0;
    }
    sleep(1);
  }

  syslog(LOG_INFO, "Shutting down...");
  ret = fire_box_stop();
  if (ret != 0) {
    syslog(LOG_WARNING, "Stop returned code: %d", ret);
  }
  syslog(LOG_INFO, "FireBox daemon stopped");
  closelog();

  return 0;
}
