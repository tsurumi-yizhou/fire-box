# Dashboard

The dashboard provides a real-time overview of the FireBox service usage and performance.

## Service Control

A prominent action bar:

- **Service Status:** (e.g., "Running", "Stopped", "Starting...")
- **Start/Stop Button:**
    - **Start:** Invokes native service manager commands to start the backend.
    - **Stop:** Invokes native service manager commands to stop the backend.
    - **Confirmation:** Stopping the service should prompt the user for confirmation if active connections exist.

## Key Performance Indicators (KPIs)

Display aggregated metrics in prominent cards:

1.  **Tokens:** Total tokens processed (split by Prompt and Completion).
2.  **Requests:** Total number of successful vs. failed requests.
3.  **Costs:** Estimated total price of the consumption (calculated based on the routing rules and provider's pricing).

## Visualization

- **Real-time Chart:** A line chart showing request volume and token throughput over a selected date range.
- **Date Range Picker:** Allows the user to select a date range for the chart. The minimum selectable unit is **one day**.
- **Metric Grids:**
    - **Total Prompt Tokens:** (e.g., "1.2M")
    - **Total Completion Tokens:** (e.g., "450k")
    - **Total Spend:** (e.g., "$12.45")
- **Refresh Control:** Auto-refresh is enabled by default at **1-second intervals**. A toggle allows disabling auto-refresh.

## Data Source
Backend's `GetMetricsSnapshot` and `GetMetricsRange` APIs.
