"""Places 0.13.1 institutional ceiling / utility furniture authoring.
All coordinates metres, +Y up, +Z front. Called by lead's isolated Blender driver.
No runtime image generation. Textures are authored and retained separately in GIMP.
"""
import math
import assetkit as k


def ceiling(variant,pass_no):
    name='suspended_ceiling_'+variant
    k.init(pass_no)
    # Actual T-grid, not an opaque ceiling slab; vacant bays remain true apertures.
    for x in [-.59,0,.59]:
        k.box('longitudinal_T_flange',(x,.052,0),(.022,.012,1.2),'paint_cream')
        if pass_no>=2:k.box('longitudinal_T_web',(x,.067,0),(.006,.03,1.2),'galvanized')
    for z in [-.59,0,.59]:
        k.box('cross_T_flange',(0,.052,z),(1.2,.012,.022),'paint_cream')
        if pass_no>=2:k.box('cross_T_web',(0,.067,z),(1.2,.03,.006),'galvanized')
    for i,(x,z) in enumerate([(-.295,-.295),(.295,-.295),(-.295,.295),(.295,.295)]):
        if variant=='exposed_grid' or (variant=='missing_tile' and i==3):continue
        role='waterstain' if variant=='stained_tiles' and i in [0,3] else 'acoustic'
        tile=k.box('mineral_fibre_tile_%d'%i,(x,.079,z),(.568,.026,.568),role,.001)
        if variant=='crooked_tile' and i==3:
            # One tile is displaced just below its flange; it still catches one edge.
            tile.location.z-=.075
            k.rotate(tile,14,'x');k.rotate(tile,-5,'z')
    if pass_no>=3:
        # Local clips add a mechanical read without inventing unrelated overhead props.
        for x,z in [(-.59,0),(.59,0),(0,-.59),(0,.59)]:
            k.box('T_grid_lock_tab',(x,.088,z),(.024,.009,.032),'brushed')
    return k.finish(name,pass_no,["1.2 m modular T-grid, 0.6 m stock bays; real empty apertures.","Mount at ceiling using measured model top Y; occupied base follows ordinary Places origin.","Crooked tile is a fixed damaged pose, not simulated."])


def light(variant,pass_no):
    name='fluorescent_'+variant
    k.init(pass_no)
    depth=.105 if variant=='surface' else .064
    if variant=='hanging':depth=.082
    # Closed body behind actual bottom aperture. Local X is long dimension.
    k.box('closed_backpan',(0,depth-.006,0),(1.2,.012,.6),'paint_cream')
    for z in [-.288,.288]:
        k.box('sidewall',(0,depth/2,z),(1.2,depth,.024),'paint_cream',.002)
    for x in [-.588,.588]:
        k.box('endwall',(x,depth/2,0),(.024,depth,.552),'paint_cream',.002)
    for z in [-.276,.276]:
        k.box('bottom_bezel_long',(0,.009,z),(1.152,.018,.024),'brushed')
    for x in [-.576,.576]:
        k.box('bottom_bezel_end',(x,.009,0),(.024,.018,.528),'brushed')
    if variant not in ['missing_diffuser','damaged']:
        k.box('recessed_prismatic_diffuser',(0,.022,0),(1.128,.008,.528),'diffuser')
    else:
        # Exposed tubes are closed cylinders, never luminous broad sheets.
        for z in [-.14,.14]:
            k.cyl('exposed_T8_tube',(0,.033,z),.018,1.07,'tube',axis='x',verts=10)
        if variant=='damaged':
            k.box('retained_diffuser_half',(-.30,.014,0),(.50,.008,.52),'diffuser')
            broken=k.box('hanging_diffuser_fragment',(.18,-.04,.18),(.34,.008,.19),'diffuser')
            k.rotate(broken,22,'x');k.rotate(broken,-7,'z')
    if pass_no>=2:
        for x in [-.54,.54]:
            for z in [-.14,.14]:
                k.box('tube_socket',(x,.036,z),(.035,.045,.055),'charcoal')
        k.box('ballast_channel',(0,depth-.025,0),(.9,.025,.075),'galvanized')
    if variant=='hanging':
        for x in [-.43,.43]:
            k.rod('steel_suspension_cable',(x,depth,0),(x,.66,0),.004,'brushed',verts=6)
            if pass_no>=2:k.box('ceiling_cable_anchor',(x,.663,0),(.08,.015,.055),'paint_cream',.002)
        if pass_no>=3:
            k.rod('power_flex',(-.35,depth,.14),(-.37,.65,.14),.008,'charcoal',verts=6)
    if pass_no>=3:
        for x in [-.49,.49]:
            k.box('diffuser_retention_clip',(x,.004,.283),(.045,.01,.017),'charcoal')
        k.box('service_label',(.4,depth+.001,.08),(.13,.003,.065),'label')
    return k.finish(name,pass_no,["1.2 x 0.6 m institutional fitting, long axis X.","Opaque closed backpan and real lens aperture. No built-in illumination or unconditional buzz.","Mount with top Y at ceiling; hanging cable anchors at measured top. Add switched runtime light source separately."])


def wall_clock(pass_no):
    name='wall_clock_institutional';k.init(pass_no)
    k.cyl('closed_dark_casing',(0,.17,0),.17,.052,'charcoal',axis='z',verts=24)
    face=k.cyl('clock_face',(0,.17,.028),.149,.004,'clockface',axis='z',verts=24)
    # Clockface has radial planar UVs, unlike a box's fitted rectangle.
    cell=k.ROLES.index('clockface');cx=cell%4;cy=cell//4
    for f in face.data.polygons:
        for li in f.loop_indices:
            v=face.data.vertices[face.data.loops[li].vertex_index].co
            u=.5+v.x/(2*.149);v2=.5-v.z/(2*.149)
            face.data.uv_layers.active.data[li].uv=((cx+.055+u*.89)/4,1-(cy+.055+v2*.89)/4)
    if pass_no>=2:
        # Hands at 10:08, a quiet fixed static pose.
        k.cyl('hand_boss',(0,.17,.037),.014,.007,'charcoal',axis='z',verts=12)
    if pass_no>=3:
        # Restrained narrow metal rim, not a ballooned rounded case.
        for a in range(12):
            t=2*math.pi*a/12
            tick=k.box('raised_hour_marker',(math.sin(t)*.126,.17+math.cos(t)*.126,.032),(.007,.016,.002),'charcoal')
            k.rotate(tick,-a*30,'z')
        k.box('rear_wall_mount',(0,.19,-.031),(.06,.06,.012),'brushed')
    return k.finish(name,pass_no,["Fixed static 10:08 pose; no real-time clock claim.","+Z front. Place rear face against wall, occupied base at mount Y."])


def wall_speaker(pass_no):
    name='wall_speaker_institutional';k.init(pass_no)
    k.box('closed_folded_steel_back',(0,.16,0),(.26,.32,.08),'paint_cream',.004)
    k.box('recessed_grille_cavity',(0,.163,.043),(.222,.268,.012),'charcoal')
    for j in range(8):
        k.box('slotted_grille_bar',(0,.058+j*.029,.052),(.218,.012,.009),'paint_cream')
    if pass_no>=2:
        k.box('lower_badge',(0,.032,.052),(.08,.014,.004),'brushed')
        for x in [-.109,.109]:
            for y in [.03,.29]:k.cyl('face_screw',(x,y,.046),.004,.006,'charcoal',axis='z',verts=6)
    if pass_no>=3:
        k.box('wall_bracket',(0,.17,-.047),(.16,.20,.014),'brushed')
        k.box('top_seam',(0,.305,.042),(.224,.004,.004),'charcoal')
    return k.finish(name,pass_no,["Passive institutional wall PA speaker, closed case; +Z front.","No automatic audio emitter, only sound if level explicitly assigns it."])


def folding_table(pass_no):
    name='folding_table_institutional';k.init(pass_no)
    k.box('laminate_tabletop',(0,.725,0),(1.8,.04,.75),'laminate',.008)
    k.box('dark_edge_band',(0,.703,0),(1.79,.01,.74),'charcoal',.003)
    # Two folding U-frames inset from table ends, splayed to support broad tabletop.
    for x in [-.60,.60]:
        for z in [-.265,.265]:
            k.rod('folding_leg',(x,.69,z*.68),(x,.022,z),.018,'brushed',verts=8)
            k.cyl('rubber_foot',(x,.018,z),.023,.036,'rubber',verts=8)
        k.rod('leg_cross_tie',(x,.55,-.22),(x,.55,.22),.014,'brushed',verts=8)
    if pass_no>=2:
        for x in [-.59,.59]:
            k.box('under_table_hinge',(x,.685,0),(.065,.036,.54),'brushed')
            k.rod('fold_lock_diagonal',(x,.39,-.19),(x*.48,.68,-.19),.009,'charcoal',verts=6)
            k.rod('fold_lock_diagonal',(x,.39,.19),(x*.48,.68,.19),.009,'charcoal',verts=6)
    if pass_no>=3:
        for x in [-.61,.61]:
            k.box('locking_tab',(x*.5,.675,0),(.045,.012,.07),'charcoal')
        k.box('underside_inventory_label',(.48,.694,0),(.17,.003,.09),'label')
    return k.finish(name,pass_no,["1.8 m laminate folding utility table, open fixed pose; readable U-frame and locking braces.","Ordinary floor-contact origin. Suggested collision covers top and legs without filling walk-under volume."])


def stackable_chair(pass_no):
    name='stackable_chair_institutional';k.init(pass_no)
    # Simple compressed sheet seat/back, metal tubular frame. No pillowy plastic shell.
    k.box('seat_pan',(0,.446,0),(.43,.036,.43),'paint_sage',.01)
    back=k.box('pressed_backrest',(0,.698,-.187),(.425,.238,.032),'paint_sage',.008)
    k.rotate(back,-8,'x')
    for x in [-.208,.208]:
        for z in [-.202,.202]:
            k.rod('splayed_tubular_leg',(x*.9,.437,z*.85),(x*1.12,.018,z*1.15),.012,'brushed',verts=8)
            k.cyl('leg_end_cap',(x*1.12,.014,z*1.15),.016,.028,'rubber',verts=8)
        k.rod('back_support_upright',(x*.89,.397,-.175),(x*.89,.814,-.218),.011,'brushed',verts=8)
    if pass_no>=2:
        for x in [-.202,.202]:
            k.rod('seat_side_rail',(x,.42,-.20),(x,.42,.2),.012,'brushed',verts=8)
        k.rod('front_seat_tie',(-.20,.417,.175),(.20,.417,.175),.01,'brushed',verts=8)
        k.rod('rear_seat_tie',(-.20,.417,-.175),(.20,.417,-.175),.01,'brushed',verts=8)
    if pass_no>=3:
        for x in [-.208,.208]:
            k.box('stacking_bumper',(x,.469,-.11),(.021,.018,.05),'rubber',.002)
        k.box('underseat_serial',(0,.426,.08),(.11,.003,.065),'label')
    return k.finish(name,pass_no,["Stackable institutional chair, compressed sheet stock and splayed steel frame, fixed single-chair pose.","0.47 m approximate footprint, 0.45 m seat height; floor-contact origin."])


BUILDERS={}
for variant in ['missing_tile','crooked_tile','stained_tiles','exposed_grid']:
    BUILDERS['suspended_ceiling_'+variant]=(lambda p,v=variant:ceiling(v,p))
for variant in ['recessed','surface','hanging','damaged','missing_diffuser']:
    BUILDERS['fluorescent_'+variant]=(lambda p,v=variant:light(v,p))
BUILDERS.update({'wall_clock_institutional':wall_clock,'wall_speaker_institutional':wall_speaker,'folding_table_institutional':folding_table,'stackable_chair_institutional':stackable_chair})
