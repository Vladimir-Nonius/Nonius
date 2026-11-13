// Разница версий только там, где они отличаются. См. local.h.

#include "local.h"
#include "internal.h"

#include <vector>

#include <BOPAlgo_MakerVolume.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepClass3d_SolidExplorer.hxx>
#include <BRep_Builder.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

namespace nonius {
namespace {

// Точка строго внутри ячейки: от точки на грани шагаем внутрь.
bool inner_point(const TopoDS_Shape& cell, gp_Pnt* out) {
    BRepClass3d_SolidClassifier inside(cell);
    for (const double step : {1e-2, 1e-3, 1e-1, 1e-4}) {
        for (TopExp_Explorer ex(cell, TopAbs_FACE); ex.More(); ex.Next()) {
            const TopoDS_Face& face = TopoDS::Face(ex.Current());
            gp_Pnt on;
            double u, v;
            if (!BRepClass3d_SolidExplorer::FindAPointInTheFace(face, on, u, v)) {
                continue;
            }
            BRepAdaptor_Surface surface(face, Standard_False);
            gp_Pnt p;
            gp_Vec du, dv;
            surface.D1(u, v, p, du, dv);
            gp_Vec normal = du.Crossed(dv);
            if (normal.Magnitude() < 1e-12) {
                continue;
            }
            normal.Normalize();
            // Нормаль грани смотрит из ячейки наружу.
            if (face.Orientation() != TopAbs_REVERSED) {
                normal.Reverse();
            }
            const gp_Pnt probe = on.Translated(normal * step);
            inside.Perform(probe, 1e-7);
            if (inside.State() == TopAbs_IN) {
                *out = probe;
                return true;
            }
        }
    }
    return false;
}

}  // namespace

bool local_diff(const TopoDS_Shape& was, const TopoDS_Shape& now, const TopoDS_Shape& before,
                const TopoDS_Shape& after, TopoDS_Shape* added, TopoDS_Shape* removed) {
    BRep_Builder builder;
    TopoDS_Compound plus, minus;
    builder.MakeCompound(plus);
    builder.MakeCompound(minus);
    *added = plus;
    *removed = minus;

    // Грани, которые есть только в одной версии: совпавшие — общие объекты.
    TopTools_IndexedMapOfShape FW, FN;
    TopExp::MapShapes(was, TopAbs_FACE, FW);
    TopExp::MapShapes(now, TopAbs_FACE, FN);
    // Два набора: грани одной версии друг с другом уже состыкованы, и
    // пересекать их между собой незачем — только «было» со «стало».
    TopoDS_Compound gone, fresh;
    builder.MakeCompound(gone);
    builder.MakeCompound(fresh);
    int count = 0;
    for (int i = 1; i <= FW.Extent(); ++i) {
        if (!FN.Contains(FW(i))) {
            builder.Add(gone, FW(i));
            ++count;
        }
    }
    for (int j = 1; j <= FN.Extent(); ++j) {
        if (!FW.Contains(FN(j))) {
            builder.Add(fresh, FN(j));
            ++count;
        }
    }
    if (count == 0) {
        return true;
    }
    TopTools_ListOfShape changed;
    changed.Append(gone);
    changed.Append(fresh);

    // Ячейки, на которые изменённые грани делят пространство. Разница версий
    // целиком состоит из них: общая грань границей разницы быть не может.
    Trace trace;
    BOPAlgo_MakerVolume cells;
    cells.SetArguments(changed);
    cells.SetIntersect(Standard_True);
    cells.SetAvoidInternalShapes(Standard_True);
    cells.SetRunParallel(Standard_True);
    cells.SetUseOBB(Standard_True);
    cells.Perform();
    trace.lap("  cells");
    if (cells.HasErrors()) {
        return false;
    }

    // Ячейка внутри «стало» и снаружи «было» — добавлена, наоборот — убрана.
    // Внутри обеих или снаружи обеих — не изменение.
    BRepClass3d_SolidClassifier in_before(before);
    BRepClass3d_SolidClassifier in_after(after);
    trace.lap("  classifiers");
    for (TopExp_Explorer ex(cells.Shape(), TopAbs_SOLID); ex.More(); ex.Next()) {
        gp_Pnt p;
        if (!inner_point(ex.Current(), &p)) {
            return false;
        }
        in_before.Perform(p, 1e-7);
        const TopAbs_State b = in_before.State();
        in_after.Perform(p, 1e-7);
        const TopAbs_State a = in_after.State();
        if ((a != TopAbs_IN && a != TopAbs_OUT) || (b != TopAbs_IN && b != TopAbs_OUT)) {
            return false;
        }
        if (a == TopAbs_IN && b == TopAbs_OUT) {
            builder.Add(plus, ex.Current());
        } else if (a == TopAbs_OUT && b == TopAbs_IN) {
            builder.Add(minus, ex.Current());
        }
    }
    trace.lap("  cells sorted");
    return true;
}

}  // namespace nonius
