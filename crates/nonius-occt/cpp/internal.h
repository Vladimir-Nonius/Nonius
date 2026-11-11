// Общее для файлов прослойки. Внутренний заголовок, не часть C-интерфейса.

#ifndef NONIUS_INTERNAL_H
#define NONIUS_INTERNAL_H

#include <chrono>
#include <cstdio>
#include <cstdlib>

#include <TopoDS_Shape.hxx>

struct NoniusShape {
    TopoDS_Shape shape;
};

namespace nonius {

// Замер шагов: NONIUS_TRACE=1 печатает в stderr, сколько шёл каждый.
struct Trace {
    const bool on = std::getenv("NONIUS_TRACE") != nullptr;
    std::chrono::steady_clock::time_point at = std::chrono::steady_clock::now();
    void lap(const char* what) {
        if (!on) {
            return;
        }
        const auto now = std::chrono::steady_clock::now();
        std::fprintf(stderr, "[nonius] %8.3f s  %s\n",
                     std::chrono::duration<double>(now - at).count(), what);
        at = now;
    }
};

}  // namespace nonius

#endif
