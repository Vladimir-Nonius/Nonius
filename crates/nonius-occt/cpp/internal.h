// Общее для файлов прослойки. Внутренний заголовок, не часть C-интерфейса.

#ifndef NONIUS_INTERNAL_H
#define NONIUS_INTERNAL_H

#include <TopoDS_Shape.hxx>

struct NoniusShape {
    TopoDS_Shape shape;
};

#endif
