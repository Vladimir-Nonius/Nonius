// Общие грани двух версий детали. См. share.h.
//
// Две версии из CAD обычно совпадают почти целиком: у пластины из 350 граней
// после правки меняются десятки. Для OpenCascade же это два чужих тела, и
// булева операция сверяет каждое ребро с каждой совпадающей гранью — на деталях
// со сплайнами это минуты. Если одинаковые грани сделать одними и теми же
// объектами, сверять остаётся только изменённое.
//
// Как: версии сопоставляются (match.h). Совпавшие грани «стало» заменяются
// гранями «было», у изменённых граней совпавшие рёбра и вершины — рёбрами и
// вершинами «было» (им добавляются кривые на этих гранях).

#include "share.h"
#include "match.h"

#include <algorithm>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Surface.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>

namespace nonius {
namespace {

bool share(const TopoDS_Shape& before, const TopoDS_Shape& after, TopoDS_Shape* result) {
    Match m;
    if (!match_shapes(before, after, &m) || m.shared_edges == 0) {
        return false;
    }

    // Изменённым граням «стало» — кривые на них у рёбер «было».
    BRep_Builder builder;
    for (int j = 1; j <= m.FB.Extent(); ++j) {
        if (m.face[j] != 0) {
            continue;
        }
        const TopoDS_Face& g = TopoDS::Face(m.FB(j));
        TopLoc_Location loc;
        Handle(Geom_Surface) surf = BRep_Tool::Surface(g, loc);
        TopTools_IndexedMapOfShape edges;
        TopExp::MapShapes(g, TopAbs_EDGE, edges);
        for (int n = 1; n <= edges.Extent(); ++n) {
            const TopoDS_Edge& eb = TopoDS::Edge(edges(n));
            const int i = m.edge[m.EB.FindIndex(eb)];
            if (i == 0) {
                continue;
            }
            // Кривые шва (c1 — для прямого ребра) кладутся на ребро в прямой ориентации.
            TopoDS_Edge ea = TopoDS::Edge(m.EA(i).Oriented(TopAbs_FORWARD));
            const double tol = std::max(BRep_Tool::Tolerance(ea), BRep_Tool::Tolerance(eb));
            double f, l;
            if (BRep_Tool::IsClosed(eb, g)) {
                Handle(Geom2d_Curve) c1 =
                    BRep_Tool::CurveOnSurface(TopoDS::Edge(eb.Oriented(TopAbs_FORWARD)), g, f, l);
                Handle(Geom2d_Curve) c2 =
                    BRep_Tool::CurveOnSurface(TopoDS::Edge(eb.Oriented(TopAbs_REVERSED)), g, f, l);
                if (c1.IsNull() || c2.IsNull()) {
                    return false;
                }
                builder.UpdateEdge(ea, c1, c2, surf, loc, tol);
            } else {
                Handle(Geom2d_Curve) c = BRep_Tool::CurveOnSurface(eb, g, f, l);
                if (c.IsNull()) {
                    return false;
                }
                builder.UpdateEdge(ea, c, surf, loc, tol);
            }
            builder.Range(ea, surf, loc, f, l);
        }
    }
    for (int j = 1; j <= m.VB.Extent(); ++j) {
        if (m.vertex[j] == 0) {
            continue;
        }
        const TopoDS_Vertex& vb = TopoDS::Vertex(m.VB(j));
        TopoDS_Vertex va = TopoDS::Vertex(m.VA(m.vertex[j]));
        if (BRep_Tool::Tolerance(vb) > BRep_Tool::Tolerance(va)) {
            builder.UpdateVertex(va, BRep_Tool::Tolerance(vb));
        }
    }

    BRepTools_ReShape reshape;
    auto put = [&reshape](const TopoDS_Shape& from, const TopoDS_Shape& to) {
        reshape.Replace(from.Oriented(TopAbs_FORWARD), to.Oriented(TopAbs_FORWARD));
    };
    for (int j = 1; j <= m.VB.Extent(); ++j) {
        if (m.vertex[j] != 0) put(m.VB(j), m.VA(m.vertex[j]));
    }
    for (int j = 1; j <= m.EB.Extent(); ++j) {
        if (m.edge[j] != 0) put(m.EB(j), m.EA(m.edge[j]));
    }
    for (int j = 1; j <= m.FB.Extent(); ++j) {
        if (m.face[j] != 0) put(m.FB(j), m.FA(m.face[j]));
    }
    TopoDS_Shape shared = reshape.Apply(after);

    // Проверка: столько же граней, и изменённые грани собраны правильно.
    TopTools_IndexedMapOfShape FR;
    TopExp::MapShapes(shared, TopAbs_FACE, FR);
    if (FR.Extent() != m.FB.Extent() || !BRepCheck_Analyzer(shared).IsValid()) {
        return false;
    }
    *result = shared;
    return true;
}

}  // namespace

bool share_faces(const TopoDS_Shape& before, const TopoDS_Shape& after, TopoDS_Shape* was,
                 TopoDS_Shape* now) {
    *was = before;
    *now = after;
    try {
        // Копия без геометрии: рёбрам копии добавляются кривые, исходник не трогаем.
        TopoDS_Shape copy = BRepBuilderAPI_Copy(before, Standard_False).Shape();
        TopoDS_Shape shared;
        if (share(copy, after, &shared)) {
            *was = copy;
            *now = shared;
            return true;
        }
    } catch (...) {
    }
    return false;
}

}  // namespace nonius
