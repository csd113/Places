"""Places institutional doors and electrical gear. Metres, Y up, Z front.
Three retained construction stages: silhouette, mechanical assembly, finish.
No raster creation; all role images come from the lead's GIMP source.
"""
import math
from mathutils import Vector, Matrix
import assetkit as a


def _hinge(objects, pivot, angle):
    pivot = Vector(a.xyz(pivot))
    mat = Matrix.Translation(pivot) @ Matrix.Rotation(math.radians(angle), 4, 'Z') @ Matrix.Translation(-pivot)
    for obj in objects:
        obj.matrix_world = mat @ obj.matrix_world


def _fastener(name, x, y, z, r=.007):
    return a.cyl(name, (x,y,z), r, .004, 'brushed', axis='z', verts=6)


def _cabinet(name, pass_no, opened=False, small=False):
    a.init(pass_no)
    w,h,d = (.46,.76,.15) if small else (.72,1.62,.34)
    stock=.018 if small else .025
    # Rear, side stock, roof and floor form a truly open-front shell.
    a.box('closed_steel_back', (0,h/2,-d/2+stock/2), (w,h,stock),'paint_sage',.003)
    for sign in (-1,1):
        a.box('folded_side', (sign*(w/2-stock/2),h/2,0), (stock,h,d),'paint_sage',.003)
    for y in (stock/2,h-stock/2):
        a.box('folded_horizontal', (0,y,0), (w-2*stock,stock,d),'paint_sage',.003)
    # Shadowed mounting board is visible inside the open variant.
    if opened or pass_no >= 2:
        a.box('internal_mounting_plate',(0,h/2,-d/2+stock+.015),(w-4*stock,h-4*stock,.018),'charcoal')
    before=set(a.gather())
    a.box('hinged_access_cover',(0,h/2,d/2+.008),(w-.008,h-.008,.021),'paint_sage',.003)
    if pass_no >= 2:
        a.box('recessed_latch',(w*.30,h*.48,d/2+.024),(.025,.092,.009),'charcoal',.002)
        a.box('latch_handle',(w*.30,h*.48,d/2+.034),(.013,.064,.014),'brushed',.002)
        a.box('electrical_warning_plate',(-w*.12,h*.77,d/2+.021),(.14,.09,.003),'label')
        if not small:
            for row in range(4):
                a.box('lower_louvre_shadow',(0,h*.18+row*.025,d/2+.020),(w*.52,.009,.003),'charcoal')
                a.box('folded_louvre_lip',(0,h*.18+row*.025+.006,d/2+.024),(w*.52,.005,.008),'paint_sage')
    if pass_no >= 3:
        for x in (-w*.40,w*.40):
            for y in (h*.065,h*.935):
                _fastener('cover_slot_screw',x,y,d/2+.022,.005 if small else .007)
    panel=[o for o in a.gather() if o not in before]
    if opened:
        _hinge(panel,(-w/2+.015,0,d/2+.008),-104)
    if pass_no >= 2:
        for y in (h*.20,h*.78):
            a.cyl('rolled_hinge',(-w/2+.010,y,d/2+.015),.010,.055,'brushed',verts=8)
        # Internals are deliberately included only on the open static piece.
        if opened:
            rows=4 if small else 6
            for row in range(rows):
                yy=h*.25+row*h*.085
                a.box('din_mounting_rail',(0,yy,-d/2+stock+.035),(w*.74,.025,.022),'brushed')
                for side in (-1,1):
                    a.box('breaker_body',(side*w*.18,yy+.018,-d/2+stock+.062),(w*.24,.052 if small else .082,.065),'paint_cream')
                    a.box('breaker_toggle',(side*w*.18,yy+.020,-d/2+stock+.100),(w*.07,.028,.014),'charcoal')
            a.box('cable_raceway',(w*.35,h*.49,-d/2+stock+.054),(.027,h*.60,.030),'paint_cream')
            if pass_no >= 3:
                for side in (-1,1):
                    a.rod('bonding_lead',(side*w*.16,h*.86,-d/2+stock+.040),(side*w*.24,h*.78,-d/2+stock+.055),.004,'rust',verts=6)
                a.box('interior_service_schedule',(-w*.24,h*.89,-d/2+stock+.029),(w*.27,h*.085,.004),'paper')
    return a.finish(name,pass_no,[
        'Static open or closed cabinet; not an animated interactable.',
        'Open-front folded-steel shell and closed rear. Open cover is hinged 104 degrees.',
        'Pass 1 shell and silhouette; pass 2 latch, hinge, warning and breaker interiors; pass 3 single-cut manufactured edges, fasteners and bonding detail.',
        'Mount wall breaker with back plane flush to wall; place floor cabinet at Y0. Open variant origin is centred under its occupied bounding box.'
    ])


def electrical_cabinet_closed(pass_no): return _cabinet('electrical_cabinet_closed',pass_no)
def electrical_cabinet_open(pass_no): return _cabinet('electrical_cabinet_open',pass_no,True)
def breaker_panel_closed(pass_no): return _cabinet('breaker_panel_closed',pass_no,False,True)
def breaker_panel_open(pass_no): return _cabinet('breaker_panel_open',pass_no,True,True)


def _leaf(center, width, height, pass_no, double=False, left=False):
    # Manufactured hollow door represented by separate stock around a real
    # upper wired-glass aperture. No opaque sheet crosses the glass.
    x=center; t=.055; ybase=.025; glassbottom=1.12; glasstop=1.90
    stile=.13 if double else .155
    role='paint_cream'
    for side in (-1,1):
        a.box('door_leaf_stile',(x+side*(width/2-stile/2),height/2+ybase,0),(stile,height,t),role,.003)
    a.box('door_leaf_lower_panel',(x,(ybase+glassbottom)/2,0),(width-2*stile,glassbottom-ybase,t),role,.003)
    a.box('door_leaf_top_rail',(x,(glasstop+height+ybase)/2,0),(width-2*stile,height+ybase-glasstop,t),role,.003)
    gw=width-2*stile
    a.box('wired_glass_pane',(x,(glassbottom+glasstop)/2,0),(gw,.78,.008),'wireglass')
    if pass_no >= 2:
        for side in (-1,1):
            # Glazing beads close the pane perimeter on both faces.
            for z in (-.032,.032):
                a.box('vertical_glazing_bead',(x+side*(gw/2-.009),(glassbottom+glasstop)/2,z),(.018,.78,.014),'brushed')
        for yy in (glassbottom+.009,glasstop-.009):
            for z in (-.032,.032):
                a.box('horizontal_glazing_bead',(x,yy,z),(gw,.018,.014),'brushed')
        # Flat kick plates and a slim lever, not swollen tubular furniture.
        for z in (-.032,.032):
            a.box('stainless_kick_plate',(x,.18,z),(width-.055,.26,.009),'brushed')
        hx=x+(-1 if left else 1)*(width*.30)
        a.box('lever_rose',(hx,1.015,.038),(.05,.15,.012),'brushed',.002)
        a.box('lever_handle',(hx+(-.035 if left else .035),1.035,.075),(.11,.018,.025),'charcoal',.003)
        # Door closer sits above the viewing aperture, clear of glazing.
        a.box('surface_closer_body',(x,height-.10,.055),(.23,.048,.05),'brushed',.003)
        a.rod('closer_arm',(x-.07,height-.10,.085),(x+.13,height-.065,.085),.008,'charcoal',verts=6)
    if pass_no >= 3:
        # Minimal hinge barrels and one identifying plate finish the construction.
        hingex=x+(-1 if left else 1)*(width/2-.008)
        for yy in (.28,1.05,1.82):
            a.cyl('door_hinge_barrel',(hingex,yy,.016),.009,.075,'brushed',verts=8)
        a.box('small_room_identifier',(x,height-.25,.031),(.16,.055,.004),'label')
        # Small corner fasteners, four per kick plate.
        for xx in (x-width*.40,x+width*.40):
            for yy in (.075,.285): _fastener('kick_plate_screw',xx,yy,.039,.004)


def _door(name,pass_no,double=False):
    a.init(pass_no)
    h=2.16; outerw=1.98 if double else 1.08; jamb=.055
    for side in (-1,1):
        a.box('institutional_steel_jamb',(side*(outerw/2-jamb/2),h/2,-.012),(jamb,h,.16),'paint_sage',.003)
    a.box('institutional_steel_header',(0,h-jamb/2,-.012),(outerw-2*jamb,jamb,.16),'paint_sage',.003)
    leafw=(outerw-2*jamb-(.012 if double else .009))/(2 if double else 1)
    if double:
        for side in (-1,1): _leaf(side*(leafw/2+.003),leafw,h-jamb-.025,pass_no,True,side<0)
    else: _leaf(0,leafw,h-jamb-.025,pass_no)
    if pass_no >= 2:
        # Recessed stops are behind the leaves; the bottom remains a walkable opening.
        for side in (-1,1): a.box('jamb_stop',(side*(outerw/2-jamb-.008),h/2,-.052),(.016,h-jamb,.028),'charcoal')
        a.box('head_stop',(0,h-jamb-.008,-.052),(outerw-2*jamb,.016,.028),'charcoal')
    if pass_no >= 3:
        for side in (-1,1):
            a.box('jamb_face_wear_strip',(side*(outerw/2-.009),h/2,.074),(.010,h-.04,.006),'brushed')
    return a.finish(name,pass_no,[
        'Static closed institutional door assembly, metres. +Z faces corridor.',
        'Real upper aperture with separately fitted wired-glass pane; no opaque slab behind glass.',
        'Open bottom jamb: level architecture or door opening owns traversal/collision, not this decorative assembly.',
        'Pass 1 stiles, rails and glazing silhouette; pass 2 kick plates, glazing beads, lever and closer; pass 3 single-cut bevels, hinges, fasteners and identifier.',
        'Rear frame plane is game Z=-0.092 m before automatic recentering; install against the wall opening, not laid over an uncut wall.'
    ])


def utility_door(pass_no): return _door('utility_door',pass_no)
def utility_double_door_wired(pass_no): return _door('utility_double_door_wired',pass_no,True)

BUILDERS={
    'electrical_cabinet_closed':electrical_cabinet_closed,
    'electrical_cabinet_open':electrical_cabinet_open,
    'breaker_panel_closed':breaker_panel_closed,
    'breaker_panel_open':breaker_panel_open,
    'utility_door':utility_door,
    'utility_double_door_wired':utility_double_door_wired,
}
