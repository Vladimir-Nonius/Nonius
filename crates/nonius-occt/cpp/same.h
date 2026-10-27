// Совпадение рёбер и граней двух версий детали. Внутренний заголовок прослойки.
//
// Обе версии выгружены одной программой, и одинаковая геометрия в них совпадает
// до последних знаков, поэтому сравнение строгое: те же параметры, те же точки.

#ifndef NONIUS_SAME_H
#define NONIUS_SAME_H

#include <vector>

#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

namespace nonius {

// Габариты ребра — для быстрого отбора пар.
struct EdgeBox {
    double lo[3];
    double hi[3];
};

EdgeBox edge_box(const TopoDS_Shape& e);
bool boxes_close(const EdgeBox& a, const EdgeBox& b);

// Та же кривая с тем же направлением и теми же параметрами.
bool same_edge(const TopoDS_Edge& a, const TopoDS_Edge& b);

// Та же поверхность в тех же границах и с той же стороной материала.
bool same_surface(const TopoDS_Face& a, const TopoDS_Face& b);

// Рёбра грани без вырожденных (точки-полюса у конусов и сфер).
std::vector<TopoDS_Shape> real_edges(const TopoDS_Face& f);

}  // namespace nonius

#endif
