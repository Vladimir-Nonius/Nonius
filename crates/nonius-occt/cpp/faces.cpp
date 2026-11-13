// Изменённые грани двух версий и что это за поверхности. См. nonius_occt.h.
//
// Булева операция отвечает «сколько материала», а человеку нужно «что
// сделали»: отверстие стало больше, плоскость сдвинули. Грани, которые не
// сопоставились (match.h), описываются числами — плоскость, цилиндр, конус,
// сфера, тор, — а слова из них складывает Rust (`changes/`).

#include <cmath>
#include <cstdlib>
#include <cstring>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <GProp_GProps.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax3.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Pln.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>

#include "internal.h"
#include "match.h"
#include "nonius_occt.h"

namespace {

void put(double out[3], const gp_XYZ& v) {
    out[0] = v.X();
    out[1] = v.Y();
    out[2] = v.Z();
}

// Ось поверхности вращения: точка, направление; смотрит ли нормаль
// поверхности от оси (у «правой» системы координат — да).
void put_axis(NoniusFace* f, const gp_Ax3& pos, bool reversed) {
    put(f->origin, pos.Location().XYZ());
    put(f->dir, pos.Direction().XYZ());
    // Нормаль грани смотрит наружу из тела. К оси — значит, материал вокруг:
    // отверстие, вогнутое скругление.
    f->inward = (reversed != !pos.Direct()) ? 1 : 0;
}

NoniusFace describe(const TopoDS_Face& face, int side) {
    NoniusFace f;
    std::memset(&f, 0, sizeof(f));
    f.side = side;
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    f.area = props.Mass();
    put(f.center, props.CentreOfMass().XYZ());

    const bool reversed = face.Orientation() == TopAbs_REVERSED;
    BRepAdaptor_Surface s(face, Standard_False);
    double u0, u1, v0, v1;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    switch (s.GetType()) {
        case GeomAbs_Plane: {
            const gp_Ax3 pos = s.Plane().Position();
            f.kind = 1;
            put(f.origin, pos.Location().XYZ());
            gp_XYZ n = pos.Direction().XYZ();
            if (reversed != !pos.Direct()) {
                n.Reverse();
            }
            put(f.dir, n);
            break;
        }
        case GeomAbs_Cylinder: {
            const gp_Cylinder c = s.Cylinder();
            f.kind = 2;
            put_axis(&f, c.Position(), reversed);
            f.radius = c.Radius();
            f.angle = u1 - u0;
            f.from = v0;
            f.to = v1;
            break;
        }
        case GeomAbs_Cone: {
            const gp_Cone c = s.Cone();
            f.kind = 3;
            put_axis(&f, c.Position(), reversed);
            f.radius = c.RefRadius();
            f.radius2 = c.SemiAngle();
            f.angle = u1 - u0;
            f.from = v0;
            f.to = v1;
            break;
        }
        case GeomAbs_Sphere: {
            const gp_Sphere c = s.Sphere();
            f.kind = 4;
            put_axis(&f, c.Position(), reversed);
            f.radius = c.Radius();
            f.angle = u1 - u0;
            break;
        }
        case GeomAbs_Torus: {
            const gp_Torus c = s.Torus();
            f.kind = 5;
            put_axis(&f, c.Position(), reversed);
            f.radius = c.MajorRadius();
            f.radius2 = c.MinorRadius();
            f.angle = u1 - u0;
            // Вдоль малого круга: по этому углу видно, выпуклое скругление
            // или вогнутое.
            f.from = v0;
            f.to = v1;
            break;
        }
        default:
            break;
    }
    return f;
}

}  // namespace

extern "C" {

int32_t nonius_changed_faces(const NoniusShape* before, const NoniusShape* after, NoniusFace** out,
                             size_t* count, NoniusShape** fresh, NoniusShape** gone) {
    *out = nullptr;
    *count = 0;
    *fresh = nullptr;
    *gone = nullptr;
    try {
        nonius::Match m;
        if (!nonius::match_shapes(before->shape, after->shape, &m)) {
            return 0;
        }
        BRep_Builder builder;
        TopoDS_Compound appeared, vanished;
        builder.MakeCompound(appeared);
        builder.MakeCompound(vanished);
        std::vector<NoniusFace> faces;
        for (int i = 1; i <= m.FA.Extent(); ++i) {
            if (!m.kept[i]) {
                const TopoDS_Face& f = TopoDS::Face(m.FA(i));
                builder.Add(vanished, f);
                faces.push_back(describe(f, 0));
            }
        }
        for (int j = 1; j <= m.FB.Extent(); ++j) {
            if (m.face[j] == 0) {
                const TopoDS_Face& f = TopoDS::Face(m.FB(j));
                builder.Add(appeared, f);
                faces.push_back(describe(f, 1));
            }
        }
        if (!faces.empty()) {
            const size_t bytes = faces.size() * sizeof(NoniusFace);
            NoniusFace* list = static_cast<NoniusFace*>(std::malloc(bytes));
            if (list == nullptr) {
                return 0;
            }
            std::memcpy(list, faces.data(), bytes);
            *out = list;
            *count = faces.size();
        }
        NoniusShape* a = new NoniusShape();
        a->shape = appeared;
        NoniusShape* v = new NoniusShape();
        v->shape = vanished;
        *fresh = a;
        *gone = v;
        return 1;
    } catch (...) {
    }
    std::free(*out);
    delete *fresh;
    delete *gone;
    *out = nullptr;
    *count = 0;
    *fresh = nullptr;
    *gone = nullptr;
    return 0;
}

void nonius_faces_free(NoniusFace* faces) { std::free(faces); }

}  // extern "C"
