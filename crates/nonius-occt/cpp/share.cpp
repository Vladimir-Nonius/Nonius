// Общие грани двух версий детали. См. share.h.
//
// Две версии из CAD обычно совпадают почти целиком: у пластины из 350 граней
// после правки меняются десятки. Для OpenCascade же это два чужих тела, и
// булева операция сверяет каждое ребро с каждой совпадающей гранью — на деталях
// со сплайнами это минуты. Если одинаковые грани сделать одними и теми же
// объектами, сверять остаётся только изменённое.
//
// Как: рёбра «стало» сравниваются с рёбрами «было» по кривой (те же параметры,
// те же точки), грани — по поверхности и набору рёбер. Совпавшие грани «стало»
// заменяются гранями «было», у изменённых граней совпавшие рёбра и вершины —
// рёбрами и вершинами «было» (им добавляются кривые на этих гранях).

#include "share.h"
#include "same.h"

#include <algorithm>
#include <vector>

#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Surface.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>

namespace nonius {
namespace {

// Отображение «стало» → «было» для вершин; false — противоречие.
bool map_vertex(const TopoDS_Shape& vb, const TopoDS_Shape& va,
                const TopTools_IndexedMapOfShape& VA, const TopTools_IndexedMapOfShape& VB,
                std::vector<int>& vmap, std::vector<int>& vback) {
    const int j = VB.FindIndex(vb);
    const int i = VA.FindIndex(va);
    if (i == 0 || j == 0) {
        return false;
    }
    if (vmap[j] == 0 && vback[i] == 0) {
        vmap[j] = i;
        vback[i] = j;
        return true;
    }
    return vmap[j] == i && vback[i] == j;
}

bool share(const TopoDS_Shape& before, const TopoDS_Shape& after, TopoDS_Shape* result) {
    TopTools_IndexedMapOfShape EA, EB, VA, VB;
    TopExp::MapShapes(before, TopAbs_EDGE, EA);
    TopExp::MapShapes(after, TopAbs_EDGE, EB);
    TopExp::MapShapes(before, TopAbs_VERTEX, VA);
    TopExp::MapShapes(after, TopAbs_VERTEX, VB);

    // Рёбра: сначала по середине (отбор по x за log n), потом по кривой.
    struct Key {
        gp_Pnt mid;
        int index;
    };
    std::vector<Key> keys;
    for (int i = 1; i <= EA.Extent(); ++i) {
        gp_Pnt mid;
        if (edge_key(TopoDS::Edge(EA(i)), &mid)) {
            keys.push_back({mid, i});
        }
    }
    std::sort(keys.begin(), keys.end(),
              [](const Key& a, const Key& b) { return a.mid.X() < b.mid.X(); });
    constexpr double kNear = 1e-4;
    std::vector<int> emap(EB.Extent() + 1, 0);
    std::vector<bool> taken(EA.Extent() + 1, false);
    std::vector<int> vmap(VB.Extent() + 1, 0);
    std::vector<int> vback(VA.Extent() + 1, 0);
    int shared_edges = 0;
    for (int j = 1; j <= EB.Extent(); ++j) {
        const TopoDS_Edge& eb = TopoDS::Edge(EB(j));
        gp_Pnt mid;
        if (!edge_key(eb, &mid)) {
            continue;
        }
        auto it = std::lower_bound(
            keys.begin(), keys.end(), mid.X() - kNear,
            [](const Key& k, double x) { return k.mid.X() < x; });
        for (; it != keys.end() && it->mid.X() <= mid.X() + kNear; ++it) {
            const int i = it->index;
            if (taken[i] || it->mid.Distance(mid) > kNear) {
                continue;
            }
            const TopoDS_Edge& ea = TopoDS::Edge(EA(i));
            if (!same_edge(ea, eb)) {
                continue;
            }
            TopoDS_Vertex af, al, bf, bl;
            TopExp::Vertices(ea, af, al);
            TopExp::Vertices(eb, bf, bl);
            if (af.IsNull() || al.IsNull() || bf.IsNull() || bl.IsNull()) {
                continue;
            }
            if (!map_vertex(bf, af, VA, VB, vmap, vback) ||
                !map_vertex(bl, al, VA, VB, vmap, vback)) {
                return false;  // вершины не сходятся — тела устроены по-разному
            }
            emap[j] = i;
            taken[i] = true;
            ++shared_edges;
            break;
        }
    }
    if (shared_edges == 0) {
        return false;
    }

    // Грани: та же поверхность и те же рёбра.
    TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
    TopExp::MapShapesAndAncestors(before, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    TopTools_IndexedMapOfShape FB;
    TopExp::MapShapes(after, TopAbs_FACE, FB);
    TopTools_IndexedMapOfShape used_faces;
    std::vector<TopoDS_Shape> twin(FB.Extent() + 1);
    for (int j = 1; j <= FB.Extent(); ++j) {
        const TopoDS_Face& fb = TopoDS::Face(FB(j));
        // Рёбра «было», в которые переходят рёбра этой грани; 0 — ребро своё.
        std::vector<int> mapped;
        for (const TopoDS_Shape& e : real_edges(fb)) {
            mapped.push_back(emap[EB.FindIndex(e)]);
        }
        if (mapped.empty() || std::count(mapped.begin(), mapped.end(), 0) > 0) {
            continue;
        }
        const int k = edge_faces.FindIndex(EA(mapped.front()));
        if (k == 0) {
            continue;
        }
        for (TopTools_ListOfShape::Iterator it(edge_faces(k)); it.More(); it.Next()) {
            const TopoDS_Face& fa = TopoDS::Face(it.Value());
            if (used_faces.Contains(fa)) {
                continue;
            }
            const std::vector<TopoDS_Shape> ea = real_edges(fa);
            if (ea.size() != mapped.size()) {
                continue;
            }
            TopTools_IndexedMapOfShape ea_set;
            for (const TopoDS_Shape& e : ea) {
                ea_set.Add(e);
            }
            bool same_edges = true;
            for (int i : mapped) {
                same_edges = same_edges && ea_set.Contains(EA(i));
            }
            // Ориентация грани берётся из её тела: у «было» и «стало» она
            // должна совпасть, иначе материал по разные стороны.
            if (same_edges && same_surface(fa, fb)) {
                twin[j] = fa;
                used_faces.Add(fa);
                break;
            }
        }
    }

    // Изменённым граням «стало» — кривые на них у рёбер «было».
    BRep_Builder builder;
    for (int j = 1; j <= FB.Extent(); ++j) {
        if (!twin[j].IsNull()) {
            continue;
        }
        const TopoDS_Face& g = TopoDS::Face(FB(j));
        TopLoc_Location loc;
        Handle(Geom_Surface) surf = BRep_Tool::Surface(g, loc);
        TopTools_IndexedMapOfShape edges;
        TopExp::MapShapes(g, TopAbs_EDGE, edges);
        for (int n = 1; n <= edges.Extent(); ++n) {
            const TopoDS_Edge& eb = TopoDS::Edge(edges(n));
            const int i = emap[EB.FindIndex(eb)];
            if (i == 0) {
                continue;
            }
            // Кривые шва (c1 — для прямого ребра) кладутся на ребро в прямой ориентации.
            TopoDS_Edge ea = TopoDS::Edge(EA(i).Oriented(TopAbs_FORWARD));
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
    for (int j = 1; j <= VB.Extent(); ++j) {
        if (vmap[j] == 0) {
            continue;
        }
        const TopoDS_Vertex& vb = TopoDS::Vertex(VB(j));
        TopoDS_Vertex va = TopoDS::Vertex(VA(vmap[j]));
        if (BRep_Tool::Tolerance(vb) > BRep_Tool::Tolerance(va)) {
            builder.UpdateVertex(va, BRep_Tool::Tolerance(vb));
        }
    }

    BRepTools_ReShape reshape;
    auto put = [&reshape](const TopoDS_Shape& from, const TopoDS_Shape& to) {
        reshape.Replace(from.Oriented(TopAbs_FORWARD), to.Oriented(TopAbs_FORWARD));
    };
    for (int j = 1; j <= VB.Extent(); ++j) {
        if (vmap[j] != 0) put(VB(j), VA(vmap[j]));
    }
    for (int j = 1; j <= EB.Extent(); ++j) {
        if (emap[j] != 0) put(EB(j), EA(emap[j]));
    }
    for (int j = 1; j <= FB.Extent(); ++j) {
        if (!twin[j].IsNull()) put(FB(j), twin[j]);
    }
    TopoDS_Shape shared = reshape.Apply(after);

    // Проверка: столько же граней, и изменённые грани собраны правильно.
    TopTools_IndexedMapOfShape FR;
    TopExp::MapShapes(shared, TopAbs_FACE, FR);
    if (FR.Extent() != FB.Extent() || !BRepCheck_Analyzer(shared).IsValid()) {
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
