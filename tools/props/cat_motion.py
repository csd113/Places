"""Cat-specific, in-place Spoonerman motion using the existing two-link legs."""
import math


def ease(t):
    t = max(0.0, min(1.0, t))
    return t*t*(3-2*t)


def mix(a, b, t):
    return a+(b-a)*t


def angle(value):
    return (value+math.pi) % math.tau-math.pi


def feet_at_rest(model):
    return {side: model.rest_global(model.index_of[f'leg_{side}_paw'])[1]
            for side in ('fl','fr','rl','rr')}


def solve_legs(model, pose, feet):
    """Planar two-bone IK, preserving bone lengths and anatomical knee direction."""
    lat, up, trans = (dict(v) for v in pose)
    for side, target in feet.items():
        names = [f'leg_{side}_{part}' for part in ('upper','lower','paw')]
        for name in names:
            lat[name] = 0.0
        hip, knee, paw = [model._posed_global(model.index_of[n],lat,up,trans)[1] for n in names]
        v1 = (knee[1]-hip[1],knee[2]-hip[2])
        v2 = (paw[1]-knee[1],paw[2]-knee[2])
        l1,l2 = math.hypot(*v1),math.hypot(*v2)
        dy,dz = target[1]-hip[1],target[2]-hip[2]
        distance = max(abs(l1-l2)+1e-6,min(math.hypot(dy,dz),l1+l2-1e-6))
        bend = math.acos(max(-1.0,min(1.0,(distance*distance-l1*l1-l2*l2)/(2*l1*l2))))
        bend *= -1 if side.startswith('f') else 1
        shoulder = math.atan2(dz,dy)-math.atan2(l2*math.sin(bend),l1+l2*math.cos(bend))
        baseline = math.atan2(v1[1],v1[0])
        rest_bend = angle(math.atan2(v2[1],v2[0])-baseline)
        lat[names[0]] = math.degrees(angle(shoulder-baseline))
        lat[names[1]] = math.degrees(angle(bend-rest_bend))
        # Compensate every ancestor's sagittal rotation so the paw pad stays
        # parallel to the floor instead of dragging toes through it.
        ancestor = model.parent.get(model.index_of[names[2]])
        total = 0.0
        while ancestor is not None:
            total += lat.get(model.names[ancestor],0.0)
            ancestor = model.parent.get(ancestor)
        lat[names[2]] = -total
    return lat,up,trans


def grounded(model, pose):
    low = min(p[1] for p in model.skinned_points(pose))
    if low < -0.0001:
        lat,up,trans = pose
        trans=dict(trans); x,y,z=trans.get('pelvis',(0,0,0))
        trans['pelvis']=(x,y-low,z)
        return lat,up,trans
    return pose


def body_pose(drop=0.0, pitch=0.0, spine=0.0, chest=0.0, head=0.0, tail=0.0, sway=0.0):
    lat={'pelvis':pitch,'spine':spine,'chest':chest,'neck':-(pitch+spine+chest)*.55,'head':head}
    up={}
    for i in range(1,9):
        lat[f'tail_0{i}']=tail*(1.0 if i==1 else .12)
        up[f'tail_0{i}']=sway*(.5 if i==1 else 1.0)
    return lat,up,{'pelvis':(0,drop,0)}


def walk_pose(model, phase):
    rest=feet_at_rest(model); feet={}
    # Lateral-sequence feline walk: hind-left, front-left, hind-right,
    # front-right. Long support phases and short low swings avoid a trot.
    for side,offset in (('rl',0.0),('fl',.25),('rr',.5),('fr',.75)):
        q=(phase-offset)%1.0
        duty=.68
        travel=.20*.6*duty
        if q<duty:
            fore=travel*(.5-q/duty); lift=0
        else:
            t=(q-duty)/(1-duty)
            fore=travel*(-.5+ease(t)); lift=.024*math.sin(math.pi*t)**2
        x,y,z=rest[side];feet[side]=(x,y+lift,z+fore)
    pose=body_pose(drop=-.015+.002*math.cos(math.tau*phase*2),
                   spine=1.5*math.sin(math.tau*phase),head=-1,
                   sway=2.5*math.sin(math.tau*phase))
    return grounded(model,solve_legs(model,pose,feet))


def run_pose(model, phase):
    rest=feet_at_rest(model);feet={}
    # Gathered hind push, then leading/trailing fore catches: a bounding
    # gallop rather than an accelerated walk. A small lead offset avoids
    # mechanical synchronous pairs.
    for side,offset in (('rl',0.0),('rr',.07),('fl',.48),('fr',.55)):
        q=(phase-offset)%1.0; duty=.36; travel=.60*.46*duty
        if q<duty:
            fore=travel*(.5-q/duty); lift=0
        else:
            t=(q-duty)/(1-duty)
            fore=travel*(-.5+ease(t));lift=.028*math.sin(math.pi*t)**2
        x,y,z=rest[side];feet[side]=(x,y+lift,z+fore)
    wave=math.sin(math.tau*phase)
    pose=body_pose(drop=-.016+.005*math.cos(math.tau*phase*2),pitch=3*wave,
                   spine=5*wave,chest=-2*wave,head=-2,tail=-16+5*wave,sway=1.5*wave)
    pose=grounded(model,solve_legs(model,pose,feet))
    # Two brief suspension phases between hind push-off and fore catch.
    for start,end in ((.43,.48),(.91,1.0)):
        if start<phase<end:
            lat,up,trans=pose;trans=dict(trans)
            x,y,z=trans['pelvis']
            trans['pelvis']=(x,y+.026*math.sin(math.pi*(phase-start)/(end-start))**2,z)
            pose=lat,up,trans
    return pose


def seated_pose(model, amount, breath=0.0):
    rest=feet_at_rest(model);feet={}
    for side,(x,y,z) in rest.items():
        # Front feet brace while the pelvis drops back over folded hocks.
        destination=z if side.startswith('f') else -.10
        feet[side]=(x,y,mix(z,destination,amount))
    pose=body_pose(drop=-.116*amount+breath*.0015,pitch=-20*amount,
                   spine=-5*amount+breath*.4,chest=-2*amount,
                   head=6*amount,tail=-18*amount,sway=5*amount)
    return grounded(model,solve_legs(model,pose,feet))


def pounce_pose(model, phase):
    # time, pelvis height, pelvis pitch, spine, fore-paw reach/lift,
    # hind-paw reach/lift, tail pitch. Horizontal root motion stays zero.
    keys=[
        (0.00,0,0,0,0,0,0,0,0),
        (.22,-.055,4,4,.005,0,.015,0,-18),
        (.32,-.065,5,5,.015,0,.022,0,-22),
        (.44,.035,-5,-4,.060,.055,-.038,.005,-8),
        (.58,.160,-4,-6,.045,.075,-.015,.080,0),
        (.70,.130,5,5,.055,.045,.015,.065,-12),
        (.82,.005,7,5,.050,0,.005,.025,-22),
        (.90,-.040,2,3,.020,0,0,0,-12),
        (1.00,0,0,0,0,0,0,0,0)]
    for a,b in zip(keys,keys[1:]):
        if phase<=b[0]:
            t=ease((phase-a[0])/(b[0]-a[0]));values=[mix(x,y,t) for x,y in zip(a[1:],b[1:])];break
    drop,pitch,spine,freach,flift,hreach,hlift,tail=values
    pose=body_pose(drop,pitch,spine,head=-(pitch+spine)*.3,tail=tail)
    feet={}
    for side,(x,y,z) in feet_at_rest(model).items():
        front=side.startswith('f')
        # Airborne foot targets travel with the body's vertical arc.
        airborne=max(0,drop)
        feet[side]=(x,y+airborne+(flift if front else hlift),z+(freach if front else hreach))
    return grounded(model,solve_legs(model,pose,feet))


def build_cat_clips(model, standing_idle):
    walk=[walk_pose(model,i/48) for i in range(49)]
    run=[run_pose(model,i/48) for i in range(49)]
    down=[seated_pose(model,ease(i/72)) for i in range(73)]
    seated=[seated_pose(model,1,math.sin(math.tau*i/90)) for i in range(91)]
    up=[seated_pose(model,1-ease(i/60)) for i in range(61)]
    pounce=[pounce_pose(model,i/96) for i in range(97)]
    # Force exact exported boundaries, including zero-rotation bind stance.
    rest=({}, {}, {})
    down[0]=up[-1]=pounce[0]=pounce[-1]=rest
    down[-1]=up[0]=seated[0]=seated[-1]
    walk[-1]=walk[0];run[-1]=run[0]
    return [('idle',4.0,standing_idle),('walk',.6,walk),('run',.46,run),
            ('sit_down',1.8,down),('sit_idle',5.0,seated),('stand_up',1.5,up),
            ('pounce',1.4,pounce)]
