// Совпадение рёбер и граней двух версий. См. same.h.

#include "same.h"

#include <algorithm>
#include <cmath>

#include <BRepAdaptor_Surface.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_Conic.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Line.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

namespace nonius {
namespace {

// Совпадение точек, мм.
constexpr double kTol = 1e-6;

bool same_param(double a, double b, double span) {
    return std::fabs(a - b) <= 1e-9 * std::max(1.0, span);
}

bool same_reals(const TColStd_Array1OfReal& a, const TColStd_Array1OfReal& b) {
    if (a.Length() != b.Length()) {
        return false;
    }
    for (int i = 0; i < a.Length(); ++i) {
        const double x = a(a.Lower() + i);
        const double y = b(b.Lower() + i);
        if (!same_param(x, y, std::fabs(x))) {
            return false;
        }
    }
    return true;
}

bool same_ints(const TColStd_Array1OfInteger& a, const TColStd_Array1OfInteger& b) {
    if (a.Length() != b.Length()) {
        return false;
    }
    for (int i = 0; i < a.Length(); ++i) {
        if (a(a.Lower() + i) != b(b.Lower() + i)) {
            return false;
        }
    }
    return true;
}

bool same_pnt(const gp_Pnt& a, const gp_Trsf& ta, const gp_Pnt& b, const gp_Trsf& tb) {
    return a.Transformed(ta).Distance(b.Transformed(tb)) <= kTol;
}

// Кривая без обрезки: обрезка задаётся параметрами ребра, они сверяются отдельно.
Handle(Geom_Curve) basis(Handle(Geom_Curve) c) {
    while (!c.IsNull() && c->IsKind(STANDARD_TYPE(Geom_TrimmedCurve))) {
        c = Handle(Geom_TrimmedCurve)::DownCast(c)->BasisCurve();
    }
    return c;
}

Handle(Geom_Surface) basis(Handle(Geom_Surface) s) {
    while (!s.IsNull() && s->IsKind(STANDARD_TYPE(Geom_RectangularTrimmedSurface))) {
        s = Handle(Geom_RectangularTrimmedSurface)::DownCast(s)->BasisSurface();
    }
    return s;
}

// Полюса и веса сплайновой кривой.
template <class C>
bool same_curve_poles(const C& a, const gp_Trsf& ta, const C& b, const gp_Trsf& tb) {
    if (a.NbPoles() != b.NbPoles()) {
        return false;
    }
    for (int i = 1; i <= a.NbPoles(); ++i) {
        if (!same_pnt(a.Pole(i), ta, b.Pole(i), tb) || !same_param(a.Weight(i), b.Weight(i), 1.0)) {
            return false;
        }
    }
    return true;
}

// Определение кривой: у прямых и конических сечений его задают точки, которые
// сверяет same_edge; у сплайнов — полюса, веса и узлы. Прочие виды кривых не
// сверяются — такие рёбра остаются своими (медленнее, но верно).
bool same_curve_definition(const Handle(Geom_Curve)& ca, const gp_Trsf& ta,
                           const Handle(Geom_Curve)& cb, const gp_Trsf& tb) {
    if (ca->DynamicType() != cb->DynamicType()) {
        return false;
    }
    if (ca->IsKind(STANDARD_TYPE(Geom_Line)) || ca->IsKind(STANDARD_TYPE(Geom_Conic))) {
        return true;
    }
    if (auto a = Handle(Geom_BSplineCurve)::DownCast(ca)) {
        auto b = Handle(Geom_BSplineCurve)::DownCast(cb);
        return a->Degree() == b->Degree() && a->IsPeriodic() == b->IsPeriodic() &&
               same_reals(a->Knots(), b->Knots()) &&
               same_ints(a->Multiplicities(), b->Multiplicities()) &&
               same_curve_poles(*a, ta, *b, tb);
    }
    if (auto a = Handle(Geom_BezierCurve)::DownCast(ca)) {
        return same_curve_poles(*a, ta, *Handle(Geom_BezierCurve)::DownCast(cb), tb);
    }
    return false;
}

// Полюса и веса сплайновой поверхности.
template <class S>
bool same_poles(const S& a, const gp_Trsf& ta, const S& b, const gp_Trsf& tb) {
    if (a.NbUPoles() != b.NbUPoles() || a.NbVPoles() != b.NbVPoles()) {
        return false;
    }
    for (int i = 1; i <= a.NbUPoles(); ++i) {
        for (int j = 1; j <= a.NbVPoles(); ++j) {
            if (!same_pnt(a.Pole(i, j), ta, b.Pole(i, j), tb) ||
                !same_param(a.Weight(i, j), b.Weight(i, j), 1.0)) {
                return false;
            }
        }
    }
    return true;
}

// То же для поверхностей: плоскость, цилиндр, конус, сфера и тор заданы точками
// из same_surface, сплайновые поверхности — полюсами, весами и узлами.
bool same_surface_definition(const TopoDS_Face& fa, const TopoDS_Face& fb, GeomAbs_SurfaceType t) {
    switch (t) {
        case GeomAbs_Plane:
        case GeomAbs_Cylinder:
        case GeomAbs_Cone:
        case GeomAbs_Sphere:
        case GeomAbs_Torus:
            return true;
        case GeomAbs_BSplineSurface:
        case GeomAbs_BezierSurface:
            break;
        default:
            return false;
    }
    TopLoc_Location la, lb;
    Handle(Geom_Surface) sa = basis(BRep_Tool::Surface(fa, la));
    Handle(Geom_Surface) sb = basis(BRep_Tool::Surface(fb, lb));
    if (sa.IsNull() || sb.IsNull() || sa->DynamicType() != sb->DynamicType()) {
        return false;
    }
    const gp_Trsf ta = la.Transformation();
    const gp_Trsf tb = lb.Transformation();
    if (auto a = Handle(Geom_BSplineSurface)::DownCast(sa)) {
        auto b = Handle(Geom_BSplineSurface)::DownCast(sb);
        return a->UDegree() == b->UDegree() && a->VDegree() == b->VDegree() &&
               a->IsUPeriodic() == b->IsUPeriodic() && a->IsVPeriodic() == b->IsVPeriodic() &&
               same_reals(a->UKnots(), b->UKnots()) && same_reals(a->VKnots(), b->VKnots()) &&
               same_ints(a->UMultiplicities(), b->UMultiplicities()) &&
               same_ints(a->VMultiplicities(), b->VMultiplicities()) &&
               same_poles(*a, ta, *b, tb);
    }
    if (auto a = Handle(Geom_BezierSurface)::DownCast(sa)) {
        return same_poles(*a, ta, *Handle(Geom_BezierSurface)::DownCast(sb), tb);
    }
    return false;
}

}  // namespace

bool edge_key(const TopoDS_Edge& e, gp_Pnt* mid) {
    if (BRep_Tool::Degenerated(e)) {
        return false;
    }
    TopLoc_Location loc;
    double f, l;
    Handle(Geom_Curve) c = BRep_Tool::Curve(e, loc, f, l);
    if (c.IsNull()) {
        return false;
    }
    *mid = c->Value(0.5 * (f + l)).Transformed(loc.Transformation());
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
    const gp_Trsf ta = la.Transformation();
    const gp_Trsf tb = lb.Transformation();
    for (int k = 0; k <= 4; ++k) {
        const double t = fa + (ea - fa) * k / 4.0;
        if (!same_pnt(ca->Value(t), ta, cb->Value(t), tb)) {
            return false;
        }
    }
    return same_curve_definition(basis(ca), ta, basis(cb), tb);
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
    return same_surface_definition(a, b, sa.GetType());
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
