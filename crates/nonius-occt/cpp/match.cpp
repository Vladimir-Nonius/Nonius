// Сопоставление двух версий детали. См. match.h.
//
// Рёбра «стало» сравниваются с рёбрами «было» по кривой (те же параметры,
// те же точки), грани — по поверхности и набору рёбер. Сравнение строгое
// (same.h): сомнительное считается разным.

#include "match.h"
#include "same.h"

#include <algorithm>

#include <TopExp.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>

namespace nonius {
namespace {

// Отображение «стало» → «было» для вершин; false — противоречие.
bool map_vertex(const TopoDS_Shape& vb, const TopoDS_Shape& va, const Match& m,
                std::vector<int>& vmap, std::vector<int>& vback) {
    const int j = m.VB.FindIndex(vb);
    const int i = m.VA.FindIndex(va);
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

// Рёбра: сначала по середине (отбор по x за log n), потом по кривой.
bool match_edges(Match& m) {
    struct Key {
        gp_Pnt mid;
        int index;
    };
    std::vector<Key> keys;
    for (int i = 1; i <= m.EA.Extent(); ++i) {
        gp_Pnt mid;
        if (edge_key(TopoDS::Edge(m.EA(i)), &mid)) {
            keys.push_back({mid, i});
        }
    }
    std::sort(keys.begin(), keys.end(),
              [](const Key& a, const Key& b) { return a.mid.X() < b.mid.X(); });
    constexpr double kNear = 1e-4;
    std::vector<bool> taken(m.EA.Extent() + 1, false);
    std::vector<int> vback(m.VA.Extent() + 1, 0);
    for (int j = 1; j <= m.EB.Extent(); ++j) {
        const TopoDS_Edge& eb = TopoDS::Edge(m.EB(j));
        gp_Pnt mid;
        if (!edge_key(eb, &mid)) {
            continue;
        }
        auto it = std::lower_bound(keys.begin(), keys.end(), mid.X() - kNear,
                                   [](const Key& k, double x) { return k.mid.X() < x; });
        for (; it != keys.end() && it->mid.X() <= mid.X() + kNear; ++it) {
            const int i = it->index;
            if (taken[i] || it->mid.Distance(mid) > kNear) {
                continue;
            }
            const TopoDS_Edge& ea = TopoDS::Edge(m.EA(i));
            if (!same_edge(ea, eb)) {
                continue;
            }
            TopoDS_Vertex af, al, bf, bl;
            TopExp::Vertices(ea, af, al);
            TopExp::Vertices(eb, bf, bl);
            if (af.IsNull() || al.IsNull() || bf.IsNull() || bl.IsNull()) {
                continue;
            }
            if (!map_vertex(bf, af, m, m.vertex, vback) ||
                !map_vertex(bl, al, m, m.vertex, vback)) {
                return false;
            }
            m.edge[j] = i;
            taken[i] = true;
            ++m.shared_edges;
            break;
        }
    }
    return true;
}

// Грани: та же поверхность и те же рёбра.
void match_faces(const TopoDS_Shape& before, Match& m) {
    TopTools_IndexedDataMapOfShapeListOfShape edge_faces;
    TopExp::MapShapesAndAncestors(before, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    for (int j = 1; j <= m.FB.Extent(); ++j) {
        const TopoDS_Face& fb = TopoDS::Face(m.FB(j));
        // Рёбра «было», в которые переходят рёбра этой грани; 0 — ребро своё.
        std::vector<int> mapped;
        for (const TopoDS_Shape& e : real_edges(fb)) {
            mapped.push_back(m.edge[m.EB.FindIndex(e)]);
        }
        if (mapped.empty() || std::count(mapped.begin(), mapped.end(), 0) > 0) {
            continue;
        }
        const int k = edge_faces.FindIndex(m.EA(mapped.front()));
        if (k == 0) {
            continue;
        }
        for (TopTools_ListOfShape::Iterator it(edge_faces(k)); it.More(); it.Next()) {
            const TopoDS_Face& fa = TopoDS::Face(it.Value());
            const int i = m.FA.FindIndex(fa);
            if (i == 0 || m.kept[i]) {
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
            for (int n : mapped) {
                same_edges = same_edges && ea_set.Contains(m.EA(n));
            }
            // Ориентация грани берётся из её тела: у «было» и «стало» она
            // должна совпасть, иначе материал по разные стороны.
            if (same_edges && same_surface(fa, fb)) {
                m.face[j] = i;
                m.kept[i] = true;
                break;
            }
        }
    }
}

}  // namespace

bool match_shapes(const TopoDS_Shape& before, const TopoDS_Shape& after, Match* out) {
    Match& m = *out;
    TopExp::MapShapes(before, TopAbs_EDGE, m.EA);
    TopExp::MapShapes(after, TopAbs_EDGE, m.EB);
    TopExp::MapShapes(before, TopAbs_VERTEX, m.VA);
    TopExp::MapShapes(after, TopAbs_VERTEX, m.VB);
    TopExp::MapShapes(before, TopAbs_FACE, m.FA);
    TopExp::MapShapes(after, TopAbs_FACE, m.FB);
    m.edge.assign(m.EB.Extent() + 1, 0);
    m.vertex.assign(m.VB.Extent() + 1, 0);
    m.face.assign(m.FB.Extent() + 1, 0);
    m.kept.assign(m.FA.Extent() + 1, false);
    m.shared_edges = 0;
    if (!match_edges(m)) {
        return false;
    }
    match_faces(before, m);
    return true;
}

}  // namespace nonius
