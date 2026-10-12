#include <iostream>
#include <memory>
#include <string>

namespace core::engine {

struct EngineConfig {
    int max_threads;
    bool debug_mode;
};

class ExecutionEngine {
public:
    ExecutionEngine(const EngineConfig& cfg);
    ~ExecutionEngine();

    bool initialize();
    void run();

private:
    EngineConfig config_;
    bool running_;
};

ExecutionEngine::ExecutionEngine(const EngineConfig& cfg) : config_(cfg), running_(false) {}

bool ExecutionEngine::initialize() {
    running_ = true;
    return true;
}

void ExecutionEngine::run() {
    std::cout << "Engine running with threads: " << config_.max_threads << std::endl;
}

std::unique_ptr<ExecutionEngine> create_engine(const EngineConfig& config) {
    return std::make_unique<ExecutionEngine>(config);
}

} // namespace core::engine
