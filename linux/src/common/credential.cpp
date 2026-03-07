/// @file credential.cpp
/// systemd-cred based credential storage.
/// Falls back to XDG_DATA_HOME/firebox/credentials/ with file-based storage
/// when systemd-creds is not available.

#include "credential.hpp"
#include <fcntl.h>
#include <spdlog/spdlog.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <array>
#include <memory>
#include <regex>
#include <sys/wait.h>
#include <unistd.h>

namespace fs = std::filesystem;

namespace firebox {
namespace {

/// Validate that a credential name contains only safe characters (alnum, dash, underscore).
bool is_valid_credential_name(const std::string& name) {
    static const std::regex valid_pattern("^[a-zA-Z0-9_-]+$");
    return !name.empty() && std::regex_match(name, valid_pattern);
}

fs::path safe_home_dir() {
    const char* home = std::getenv("HOME");
    if (home && home[0] != '\0') return fs::path(home);
    // Fallback to /tmp if HOME is not set (e.g., in containers)
    spdlog::warn("HOME environment variable not set, using /tmp");
    return fs::path("/tmp");
}

fs::path credentials_dir() {
    const char* xdg = std::getenv("XDG_DATA_HOME");
    fs::path base = (xdg && xdg[0] != '\0')
        ? fs::path(xdg)
        : safe_home_dir() / ".local" / "share";
    auto dir = base / "firebox" / "credentials";
    fs::create_directories(dir);
    // Restrictive permissions
    fs::permissions(dir, fs::perms::owner_all, fs::perm_options::replace);
    return dir;
}

/// Execute a child process with explicit argv, writing input_data to its stdin,
/// and capturing stdout. No shell involved — immune to injection.
/// Returns {exit_status, stdout_content}.
std::pair<int, std::string> exec_with_pipe(
    const std::vector<std::string>& argv,
    const std::string& input_data = {}) {

    if (argv.empty()) return {-1, {}};

    int stdin_pipe[2] = {-1, -1};
    int stdout_pipe[2] = {-1, -1};

    if (pipe(stdin_pipe) != 0 || pipe(stdout_pipe) != 0) {
        return {-1, {}};
    }

    pid_t pid = fork();
    if (pid < 0) {
        close(stdin_pipe[0]); close(stdin_pipe[1]);
        close(stdout_pipe[0]); close(stdout_pipe[1]);
        return {-1, {}};
    }

    if (pid == 0) {
        // Child process
        close(stdin_pipe[1]);   // close write end of stdin pipe
        close(stdout_pipe[0]);  // close read end of stdout pipe
        dup2(stdin_pipe[0], STDIN_FILENO);
        dup2(stdout_pipe[1], STDOUT_FILENO);
        close(stdin_pipe[0]);
        close(stdout_pipe[1]);

        // Redirect stderr to /dev/null
        int devnull = open("/dev/null", O_WRONLY);
        if (devnull >= 0) { dup2(devnull, STDERR_FILENO); close(devnull); }

        // Build C-style argv
        std::vector<const char*> c_argv;
        c_argv.reserve(argv.size() + 1);
        for (auto& a : argv) c_argv.push_back(a.c_str());
        c_argv.push_back(nullptr);

        execvp(c_argv[0], const_cast<char* const*>(c_argv.data()));
        _exit(127); // exec failed
    }

    // Parent process
    close(stdin_pipe[0]);   // close read end of stdin pipe
    close(stdout_pipe[1]);  // close write end of stdout pipe

    // Write input data to child's stdin
    if (!input_data.empty()) {
        const char* data = input_data.data();
        size_t remaining = input_data.size();
        while (remaining > 0) {
            auto written = write(stdin_pipe[1], data, remaining);
            if (written <= 0) break;
            data += written;
            remaining -= static_cast<size_t>(written);
        }
    }
    close(stdin_pipe[1]);

    // Read stdout
    std::string output;
    std::array<char, 4096> buf;
    while (true) {
        auto n = read(stdout_pipe[0], buf.data(), buf.size());
        if (n <= 0) break;
        output.append(buf.data(), static_cast<size_t>(n));
    }
    close(stdout_pipe[0]);

    // Wait for child
    int status = 0;
    waitpid(pid, &status, 0);
    int exit_code = WIFEXITED(status) ? WEXITSTATUS(status) : -1;

    // Trim trailing newlines
    while (!output.empty() && output.back() == '\n') output.pop_back();
    return {exit_code, std::move(output)};
}

bool has_systemd_creds() {
    static int cached = -1;
    if (cached < 0) {
        auto [rc, _] = exec_with_pipe({"which", "systemd-creds"});
        cached = (rc == 0) ? 1 : 0;
    }
    return cached == 1;
}

} // namespace

bool credential_store(const std::string& name, const std::string& value) {
    if (!is_valid_credential_name(name)) {
        spdlog::error("Credential name '{}' contains invalid characters", name);
        return false;
    }

    auto path = credentials_dir() / name;

    if (has_systemd_creds()) {
        // Use exec_with_pipe to avoid shell injection — value is piped via stdin
        auto [rc, _] = exec_with_pipe(
            {"systemd-creds", "encrypt", "--name=" + name, "-", path.string()},
            value);
        if (rc == 0) {
            spdlog::info("Credential '{}' stored via systemd-creds", name);
            return true;
        }
        spdlog::warn("systemd-creds encrypt failed for '{}', falling back to file", name);
    }

    // Fallback: plain file with restrictive permissions
    std::ofstream ofs(path, std::ios::trunc);
    if (!ofs) return false;
    ofs << value;
    ofs.close();
    fs::permissions(path, fs::perms::owner_read | fs::perms::owner_write,
                    fs::perm_options::replace);
    spdlog::info("Credential '{}' stored as file", name);
    return true;
}

std::string credential_load(const std::string& name) {
    if (!is_valid_credential_name(name)) {
        spdlog::error("Credential name '{}' contains invalid characters", name);
        return {};
    }

    auto path = credentials_dir() / name;
    if (!fs::exists(path)) return {};

    if (has_systemd_creds()) {
        auto [rc, result] = exec_with_pipe(
            {"systemd-creds", "decrypt", "--name=" + name, path.string(), "-"});
        if (rc == 0 && !result.empty()) return result;
        spdlog::warn("systemd-creds decrypt failed for '{}', trying plain read", name);
    }

    std::ifstream ifs(path);
    if (!ifs) return {};
    return std::string(std::istreambuf_iterator<char>(ifs),
                       std::istreambuf_iterator<char>());
}

bool credential_delete(const std::string& name) {
    if (!is_valid_credential_name(name)) return false;
    auto path = credentials_dir() / name;
    std::error_code ec;
    return fs::remove(path, ec);
}

} // namespace firebox
