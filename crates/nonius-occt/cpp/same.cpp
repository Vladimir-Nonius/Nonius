// Совпадение рёбер и граней двух версий. См. same.h.

#include "same.h"

#include <algorithm>
#include <cmath>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom_Curve.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

namespace nonius {
namespace {

// Совпадение точек, мм.
constexpr double kTol = 1e-6;

bool same_param(double a, double b, double span) {
    return std::fabs(a - b) <= 1e-9 * std::max(1.0, span);
}

}  // namespace

EdgeBox edge_box(const TopoDS_Shape& e) {
    Bnd_Box b;
    BRepBndLib::Add(e, b);
    EdgeBox r{};
    if (!b.IsVoid()) {
        b.Get(r.lo[0], r.lo[1], r.lo[2], r.hi[0], r.hi[1], r.hi[2]);
    }
    return r;
}

bool boxes_close(const EdgeBox& a, const EdgeBox& b) {
    for (int k = 0; k < 3; ++k) {
        if (std::fabs(a.lo[k] - b.lo[k]) > 1e-4 || std::fabs(a.hi[k] - b.hi[k]) > 1e-4) {
            return false;
        }
    }
    return true;
}

bool same_edge(const TopoDS_Edge& a, const TopoDS_Edge& b) {
    if (BRep_Tool::Degenerated(a) || BRep_Tool::Degenerated(b)) {
        return false;
    }
    TopLoc_Location la, lb;
    double fa, ea, fb, eb;
    Handle(Geom_Curve) ca = BRep_Tool::Curve(a, la, fa, ea);
    Handle(Geom_Curve) cb = BRep_Tool::Curve(b, lb, fb, eb);
    if (ca.IsNull() || cb.IsNull()) {
        return false;
    }
    const double span = std::fabs(ea - fa);
    if (!same_param(fa, fb, span) || !same_param(ea, eb, span)) {
        return false;
    }
    for (int k = 0; k <= 4; ++k) {
        const double t = fa + (ea - fa) * k / 4.0;
        gp_Pnt pa = ca->Value(t).Transformed(la.Transformation());
        gp_Pnt pb = cb->Value(t).Transformed(lb.Transformation());
        if (pa.Distance(pb) > kTol) {
            return false;
        }
    }
    return true;
}

bool same_surface(const TopoDS_Face& a, const TopoDS_Face& b) {
    if (a.Orientation() != b.Orientation()) {
        return false;
    }
    BRepAdaptor_Surface sa(a, Standard_False);
    BRepAdaptor_Surface sb(b, Standard_False);
    if (sa.GetType() != sb.GetType()) {
        return false;
    }
    double au0, au1, av0, av1, bu0, bu1, bv0, bv1;
    BRepTools::UVBounds(a, au0, au1, av0, av1);
    BRepTools::UVBounds(b, bu0, bu1, bv0, bv1);
    const double su = au1 - au0;
    const double sv = av1 - av0;
    if (!same_param(au0, bu0, su) || !same_param(au1, bu1, su) || !same_param(av0, bv0, sv) ||
        !same_param(av1, bv1, sv)) {
        return false;
    }
    for (int i = 0; i <= 4; ++i) {
        for (int j = 0; j <= 4; ++j) {
            const double u = au0 + su * i / 4.0;
            const double v = av0 + sv * j / 4.0;
            if (sa.Value(u, v).Distance(sb.Value(u, v)) > kTol) {
                return false;
            }
        }
    }
    return true;
}

std::vector<TopoDS_Shape> real_edges(const TopoDS_Face& f) {
    TopTools_IndexedMapOfShape m;
    TopExp::MapShapes(f, TopAbs_EDGE, m);
    std::vector<TopoDS_Shape> out;
    for (int i = 1; i <= m.Extent(); ++i) {
        if (!BRep_Tool::Degenerated(TopoDS::Edge(m(i)))) {
            out.push_back(m(i));
        }
    }
    return out;
}

}  // namespace nonius
