// Контрольные числа тела: объём, площадь, центр масс, габариты. См. nonius_occt.h.

#include <cmath>
#include <cstring>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Domain.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepGProp_Vinert.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <OSD_Parallel.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
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

// Объём — тоже сумма по граням: так его считает и сам OpenCascade
// (BRepGProp.cxx, volumeProperties), только по одной. Здесь грани считаются
// параллельно: на детали со сплайнами это секунда против долей секунды.
// Вклад грани берётся относительно одной общей точки — как там, средней по
// вершинам, — иначе вклады не складываются.
double volume_of(const TopoDS_Shape& s, gp_Pnt* center) {
    gp_XYZ sum(0, 0, 0);
    int vertices = 0;
    for (TopExp_Explorer ex(s, TopAbs_VERTEX); ex.More(); ex.Next(), ++vertices) {
        sum += BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())).XYZ();
    }
    std::vector<TopoDS_Face> faces;
    bool plain = vertices > 0;
    TopLoc_Location loc;
    for (TopExp_Explorer ex(s, TopAbs_FACE); ex.More(); ex.Next()) {
        const TopoDS_Face& f = TopoDS::Face(ex.Current());
        if (f.Orientation() != TopAbs_FORWARD && f.Orientation() != TopAbs_REVERSED) {
            continue;
        }
        // Грань без поверхности (одна сетка) — пусть считает OpenCascade.
        plain = plain && !BRep_Tool::Surface(f, loc).IsNull();
        faces.push_back(f);
    }
    GProp_GProps volume;
    if (plain) {
        const gp_Pnt about(sum / vertices);
        std::vector<GProp_GProps> parts(faces.size());
        std::vector<char> ok(faces.size(), 1);
        OSD_Parallel::For(0, static_cast<int>(faces.size()), [&](int i) {
            try {
                BRepGProp_Face face;
                BRepGProp_Domain domain;
                BRepGProp_Vinert one;
                one.SetLocation(about);
                face.Load(faces[i]);
                if (faces[i].NbChildren() == 0) {
                    one.Perform(face);
                } else {
                    domain.Init(faces[i]);
                    one.Perform(face, domain);
                }
                parts[i] = one;
            } catch (...) {
                ok[i] = 0;
            }
        });
        for (size_t i = 0; i < parts.size() && plain; ++i) {
            if (ok[i] != 0) {
                volume.Add(parts[i]);
            } else {
                plain = false;
            }
        }
    }
    if (!plain) {
        volume = GProp_GProps();
        BRepGProp::VolumeProperties(s, volume);
    }
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
        TopTools_IndexedMapOfShape edges;
        TopExp::MapShapes(shape->shape, TopAbs_EDGE, edges);
        for (int i = 1; i <= edges.Extent(); ++i) {
            BRepBndLib::AddOptimal(edges(i), box, Standard_False, Standard_False);
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
