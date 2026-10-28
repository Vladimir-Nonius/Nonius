// Контрольные числа тела: объём, площадь, центр масс, габариты. См. nonius_occt.h.

#include <cmath>
#include <cstring>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepGProp.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <OSD_Parallel.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

#include "internal.h"
#include "nonius_occt.h"

namespace {

uint32_t count(const TopoDS_Shape& shape, TopAbs_ShapeEnum kind) {
    uint32_t n = 0;
    for (TopExp_Explorer ex(shape, kind); ex.More(); ex.Next()) {
        ++n;
    }
    return n;
}

// Площадь и точные габариты — суммы по граням, грани считаются параллельно:
// на деталях со сплайнами точные габариты — секунды. Габариты те же, что у
// BRepBndLib::AddOptimal для всего тела: он тоже объединяет габариты граней.
struct Sums {
    double area = 0;
    Bnd_Box box;
};

bool face_sums(const TopoDS_Shape& s, Sums* out) {
    std::vector<TopoDS_Shape> faces;
    for (TopExp_Explorer ex(s, TopAbs_FACE); ex.More(); ex.Next()) {
        faces.push_back(ex.Current());
    }
    struct One {
        double area = 0;
        Bnd_Box box;
        bool ok = true;
    };
    std::vector<One> parts(faces.size());
    OSD_Parallel::For(0, static_cast<int>(faces.size()), [&](int i) {
        One& p = parts[i];
        try {
            GProp_GProps a;
            BRepGProp::SurfaceProperties(faces[i], a);
            p.area = a.Mass();
            BRepBndLib::AddOptimal(faces[i], p.box, Standard_False, Standard_False);
        } catch (...) {
            p.ok = false;
        }
    });
    for (const One& p : parts) {
        if (!p.ok) {
            return false;
        }
        out->area += p.area;
        out->box.Add(p.box);
    }
    // Рёбра и вершины вне граней (у проволочных тел).
    for (TopExp_Explorer ex(s, TopAbs_EDGE, TopAbs_FACE); ex.More(); ex.Next()) {
        BRepBndLib::AddOptimal(ex.Current(), out->box, Standard_False, Standard_False);
    }
    for (TopExp_Explorer ex(s, TopAbs_VERTEX, TopAbs_EDGE); ex.More(); ex.Next()) {
        BRepBndLib::AddOptimal(ex.Current(), out->box, Standard_False, Standard_False);
    }
    return true;
}

double volume_of(const TopoDS_Shape& s, gp_Pnt* center) {
    GProp_GProps volume;
    BRepGProp::VolumeProperties(s, volume);
    if (center != nullptr) {
        *center = volume.CentreOfMass();
    }
    return std::fabs(volume.Mass());
}

}  // namespace

extern "C" {

int32_t nonius_shape_props(const NoniusShape* shape, NoniusProps* out) {
    std::memset(out, 0, sizeof(NoniusProps));
    try {
        const TopoDS_Shape& s = shape->shape;
        gp_Pnt c;
        out->volume = volume_of(s, &c);
        if (out->volume > 0) {
            out->center[0] = c.X();
            out->center[1] = c.Y();
            out->center[2] = c.Z();
        }
        Sums sums;
        if (!face_sums(s, &sums)) {
            return 0;
        }
        out->area = sums.area;
        if (!sums.box.IsVoid()) {
            out->has_bbox = 1;
            sums.box.Get(out->bbox_min[0], out->bbox_min[1], out->bbox_min[2], out->bbox_max[0],
                         out->bbox_max[1], out->bbox_max[2]);
        }
        out->solids = count(s, TopAbs_SOLID);
        out->faces = count(s, TopAbs_FACE);
        return 1;
    } catch (...) {
        return 0;
    }
}

double nonius_shape_volume(const NoniusShape* shape) {
    try {
        return volume_of(shape->shape, nullptr);
    } catch (...) {
        return 0.0;
    }
}

uint32_t nonius_shape_solid_count(const NoniusShape* shape) {
    try {
        return count(shape->shape, TopAbs_SOLID);
    } catch (...) {
        return 0;
    }
}

int32_t nonius_shape_rough_bbox(const NoniusShape* shape, double min[3], double max[3]) {
    try {
        // По рёбрам: кривые считаются быстро и точно, а грани между рёбрами
        // выпирают редко и немного — для допуска сетки этого хватает.
        Bnd_Box box;
        for (TopExp_Explorer ex(shape->shape, TopAbs_EDGE); ex.More(); ex.Next()) {
            BRepBndLib::AddOptimal(ex.Current(), box, Standard_False, Standard_False);
        }
        if (box.IsVoid()) {
            BRepBndLib::Add(shape->shape, box);
        }
        if (box.IsVoid()) {
            return 0;
        }
        box.Get(min[0], min[1], min[2], max[0], max[1], max[2]);
        return 1;
    } catch (...) {
        return 0;
    }
}

}  // extern "C"
