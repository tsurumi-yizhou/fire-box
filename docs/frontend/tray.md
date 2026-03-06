# Tray & Lifecycle

The FireBox user interface provides a lightweight control point for common user interactions.

## Tray/Status Menu

The system provides a persistent menu accessible from the user's system toolbar. This menu remains visible as long as the GUI is running.

### Menu Items

1.  **Open FireBox:**
    *   **Action:** Launches or focuses the main Dashboard window.
2.  **Close:**
    *   **Action:** Exits the GUI application. 
    *   **Note:** This does **NOT** stop the background service.

## Window Lifecycle

The frontend GUI window follows a "Close to Hide" pattern.

1.  **Launch:** The GUI can be launched from the tray menu or the system application menu.
2.  **Closing the Window:** Clicking the "X" hides the interface; it does not terminate the backend service.
3.  **Service Control:** The operational state of the backend (Start/Stop) is managed via a dedicated button within the GUI (see Dashboard/Settings).


